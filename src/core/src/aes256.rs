//! AES-256 PDF encryption: key setup and the cipher (ISO 32000-2 §7.6.4, V=5 R=6).
//!
//! The pass that rewrites the file around these keys is in `rewrite.rs`, and
//! `encrypt.rs` ties the two together.
//!
//! Nothing here reads a random-number generator. The host sends `entropy` with the
//! request, as it sends `now`, and every random value below (the file key, the salts,
//! the file ID and each IV) is cut from it, so the same entropy gives the same file and
//! a test can fix it.

use cbc::cipher::block_padding::{NoPadding, Pkcs7};
use cbc::cipher::generic_array::GenericArray;
use cbc::cipher::{BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit};
use sha2::{Digest, Sha256, Sha384, Sha512};

type Aes128CbcEncryptor = cbc::Encryptor<aes::Aes128>;
type Aes256CbcEncryptor = cbc::Encryptor<aes::Aes256>;

/// The least entropy the host must send: one 256-bit key's worth.
pub(crate) const MIN_ENTROPY_BYTES: usize = 32;

/// Longest password the standard security handler reads (ISO 32000-2 §7.6.4.3.3).
const MAX_PASSWORD_BYTES: usize = 127;

/// Expands the host's entropy into as many bytes as the file needs: block *n* is the
/// SHA-256 of the entropy followed by *n* as eight big-endian bytes.
pub(crate) struct EntropyStream {
    seed:    Vec<u8>,
    counter: u64,
    block:   [u8; 32],
    used:    usize,
}

impl EntropyStream {
    /// Fails when the host sent less than [`MIN_ENTROPY_BYTES`].
    pub(crate) fn new(entropy: &[u8]) -> Result<Self, String> {
        if entropy.len() < MIN_ENTROPY_BYTES {
            return Err(format!(
                "AES-256 encryption needs at least {MIN_ENTROPY_BYTES} bytes of entropy from the host, got {}",
                entropy.len()
            ));
        }
        Ok(Self { seed: entropy.to_vec(), counter: 0, block: [0; 32], used: 32 })
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for byte in &mut out {
            if self.used == self.block.len() {
                self.refill();
            }
            *byte = self.block[self.used];
            self.used += 1;
        }
        out
    }

    fn refill(&mut self) {
        let mut hasher = Sha256::new();
        hasher.update(&self.seed);
        hasher.update(self.counter.to_be_bytes());
        self.block = hasher.finalize().into();
        self.counter += 1;
        self.used = 0;
    }
}

/// The entries of the `/Encrypt` dictionary that depend on the passwords and the key.
pub(crate) struct KeyEntries {
    /// `/O`: owner hash, validation salt, key salt.
    pub o:     [u8; 48],
    /// `/U`: user hash, validation salt, key salt.
    pub u:     [u8; 48],
    /// `/OE`: the file key, encrypted under the owner password.
    pub oe:    [u8; 32],
    /// `/UE`: the file key, encrypted under the user password.
    pub ue:    [u8; 32],
    /// `/Perms`: `/P` repeated, encrypted, so a reader can detect a changed `/P`.
    pub perms: [u8; 16],
}

/// Encrypts the strings and streams of a file under its one file key.
pub(crate) struct ObjectCipher {
    key: [u8; 32],
    ivs: EntropyStream,
}

impl ObjectCipher {
    /// A fresh IV followed by `plain` under AES-256-CBC with PKCS#7 padding, which is
    /// how the standard handler stores a string or a stream: 17 to 32 bytes longer.
    pub(crate) fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let iv: [u8; 16] = self.ivs.take();
        // The key and the IV have the lengths the cipher takes, so `into` cannot panic.
        let encryptor = Aes256CbcEncryptor::new((&self.key).into(), (&iv).into());
        let mut sealed = iv.to_vec();
        sealed.extend(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plain));
        sealed
    }
}

/// Everything a file needs to be written under AES-256.
pub(crate) struct Aes256Setup {
    pub entries: KeyEntries,
    pub file_id: [u8; 16],
    pub cipher:  ObjectCipher,
}

/// Works out the `/Encrypt` entries, the file ID and the cipher for one file.
///
/// An empty owner password falls back to the user password, as in the RC4 handler.
/// Passwords are used as UTF-8 bytes, cut at 127; the SASLprep step the standard asks
/// for is not done, so a password with characters that SASLprep changes may not open
/// in every reader.
pub(crate) fn setup(user_password: &str, owner_password: &str, p: i32, entropy: &[u8]) -> Result<Aes256Setup, String> {
    let mut stream = EntropyStream::new(entropy)?;
    let file_key: [u8; 32]       = stream.take();
    let user_validation_salt     = stream.take::<8>();
    let user_key_salt            = stream.take::<8>();
    let owner_validation_salt    = stream.take::<8>();
    let owner_key_salt           = stream.take::<8>();
    let perms_filler: [u8; 4]    = stream.take();
    let file_id: [u8; 16]        = stream.take();

    let user_bytes  = password_bytes(user_password);
    let owner_bytes = password_bytes(if owner_password.is_empty() { user_password } else { owner_password });

    let u  = hash_with_salts(&hash_r6(user_bytes, &user_validation_salt, &[]), &user_validation_salt, &user_key_salt);
    let ue = wrap_file_key(&hash_r6(user_bytes, &user_key_salt, &[]), &file_key);
    let o  = hash_with_salts(&hash_r6(owner_bytes, &owner_validation_salt, &u), &owner_validation_salt, &owner_key_salt);
    let oe = wrap_file_key(&hash_r6(owner_bytes, &owner_key_salt, &u), &file_key);

    Ok(Aes256Setup {
        entries: KeyEntries { o, u, oe, ue, perms: seal_perms(&file_key, p, perms_filler) },
        file_id,
        cipher:  ObjectCipher { key: file_key, ivs: stream },
    })
}

fn password_bytes(password: &str) -> &[u8] {
    &password.as_bytes()[..password.len().min(MAX_PASSWORD_BYTES)]
}

/// The 48 bytes of `/O` and `/U`: a hash, then the two 8-byte salts.
fn hash_with_salts(hash: &[u8; 32], validation_salt: &[u8; 8], key_salt: &[u8; 8]) -> [u8; 48] {
    let mut out = [0u8; 48];
    out[..32].copy_from_slice(hash);
    out[32..40].copy_from_slice(validation_salt);
    out[40..].copy_from_slice(key_salt);
    out
}

/// `/UE` and `/OE`: the file key under AES-256-CBC with a zero IV and no padding.
fn wrap_file_key(wrapping_key: &[u8; 32], file_key: &[u8; 32]) -> [u8; 32] {
    let encryptor = Aes256CbcEncryptor::new(wrapping_key.into(), (&[0u8; 16]).into());
    let wrapped = encryptor.encrypt_padded_vec_mut::<NoPadding>(file_key);
    let mut out = [0u8; 32];
    out.copy_from_slice(&wrapped);
    out
}

/// `/Perms`: `/P` as four little-endian bytes, four `FF` bytes, `T` (metadata is
/// encrypted), `adb`, and four more bytes, as one AES-256 block in ECB mode.
fn seal_perms(file_key: &[u8; 32], p: i32, filler: [u8; 4]) -> [u8; 16] {
    let mut plain = [0u8; 16];
    plain[..4].copy_from_slice(&p.to_le_bytes());
    plain[4..8].fill(0xFF);
    plain[8] = b'T';
    plain[9..12].copy_from_slice(b"adb");
    plain[12..].copy_from_slice(&filler);

    let mut block = GenericArray::from(plain);
    aes::Aes256::new(file_key.into()).encrypt_block(&mut block);
    block.into()
}

/// The password hash of R=6 (ISO 32000-2 Algorithm 2.B): rounds of AES-128-CBC and a
/// SHA-2 hash chosen by the result, run at least 64 times. `user_key` is empty for the
/// user password and the 48 bytes of `/U` for the owner password.
fn hash_r6(password: &[u8], salt: &[u8], user_key: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(user_key);
    let mut k: Vec<u8> = hasher.finalize().to_vec();

    let mut round = 0i32;
    let mut last_byte = 0u8;
    while round < 64 || i32::from(last_byte) > round - 32 {
        let mut repeated = Vec::with_capacity((password.len() + k.len() + user_key.len()) * 64);
        for _ in 0..64 {
            repeated.extend_from_slice(password);
            repeated.extend_from_slice(&k);
            repeated.extend_from_slice(user_key);
        }
        // `k` is 32, 48 or 64 bytes, so its first 16 bytes are the key and the next 16 the IV.
        let encryptor = Aes128CbcEncryptor::new(k[..16].into(), k[16..32].into());
        let encrypted = encryptor.encrypt_padded_vec_mut::<NoPadding>(&repeated);

        // 256 is 1 modulo 3, so the first 16 bytes as one big-endian number are
        // congruent to their sum.
        let selector = encrypted[..16].iter().map(|&b| u32::from(b)).sum::<u32>() % 3;
        k = match selector {
            0 => Sha256::digest(&encrypted).to_vec(),
            1 => Sha384::digest(&encrypted).to_vec(),
            _ => Sha512::digest(&encrypted).to_vec(),
        };
        last_byte = encrypted[encrypted.len() - 1];
        round += 1;
    }

    let mut out = [0u8; 32];
    out.copy_from_slice(&k[..32]);
    out
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use cbc::cipher::BlockDecryptMut;

    use super::*;

    type Aes256CbcDecryptor = cbc::Decryptor<aes::Aes256>;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }

    fn entropy(fill: u8) -> Vec<u8> {
        vec![fill; 48]
    }

    // FIPS 197 appendix C.3: AES-256 on one block.
    #[test]
    fn aes_256_matches_the_fips_197_vector() {
        let key = unhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let mut block = GenericArray::clone_from_slice(&unhex("00112233445566778899aabbccddeeff"));
        aes::Aes256::new(GenericArray::from_slice(&key)).encrypt_block(&mut block);
        assert_eq!(hex(&block), "8ea2b7ca516745bfeafc49904b496089");
    }

    // NIST SP 800-38A F.2.5: CBC-AES256.Encrypt, first block.
    #[test]
    fn aes_256_cbc_matches_the_sp_800_38a_vector() {
        let key = unhex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4");
        let iv  = unhex("000102030405060708090a0b0c0d0e0f");
        let plain = unhex("6bc1bee22e409f96e93d7e117393172a");
        let encryptor = Aes256CbcEncryptor::new(GenericArray::from_slice(&key), GenericArray::from_slice(&iv));
        let sealed = encryptor.encrypt_padded_vec_mut::<NoPadding>(&plain);
        assert_eq!(hex(&sealed), "f58c4c04d6e5f1ba779eabfb5f7bfbd6");
    }

    // FIPS 180-4 examples: the message "abc".
    #[test]
    fn sha_2_matches_the_fips_180_4_vectors() {
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            hex(&Sha384::digest(b"abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hex(&Sha512::digest(b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn entropy_shorter_than_32_bytes_is_refused() {
        let error = setup("", "owner", -4, &[7u8; 31]).err().expect("short entropy must be refused");
        assert!(error.contains("at least 32 bytes"), "{error}");
        assert!(error.contains("got 31"), "{error}");
    }

    #[test]
    fn the_same_entropy_gives_the_same_setup() {
        let a = setup("user", "owner", -4, &entropy(1)).unwrap();
        let b = setup("user", "owner", -4, &entropy(1)).unwrap();
        assert_eq!(a.entries.u, b.entries.u);
        assert_eq!(a.entries.o, b.entries.o);
        assert_eq!(a.entries.perms, b.entries.perms);
        assert_eq!(a.file_id, b.file_id);
    }

    #[test]
    fn different_entropy_gives_a_different_key_and_file_id() {
        let a = setup("user", "owner", -4, &entropy(1)).unwrap();
        let b = setup("user", "owner", -4, &entropy(2)).unwrap();
        assert_ne!(a.entries.ue, b.entries.ue);
        assert_ne!(a.file_id, b.file_id);
    }

    #[test]
    fn user_and_owner_passwords_both_unwrap_the_file_key() {
        let setup = setup("user", "owner", -4, &entropy(3)).unwrap();
        let unwrap = |wrapped: &[u8; 32], password: &[u8], key_salt: &[u8], user_key: &[u8]| {
            let key = hash_r6(password, key_salt, user_key);
            let decryptor = Aes256CbcDecryptor::new((&key).into(), (&[0u8; 16]).into());
            decryptor.decrypt_padded_vec_mut::<NoPadding>(wrapped).unwrap()
        };
        let by_user  = unwrap(&setup.entries.ue, b"user", &setup.entries.u[40..], &[]);
        let by_owner = unwrap(&setup.entries.oe, b"owner", &setup.entries.o[40..], &setup.entries.u);
        assert_eq!(by_user, setup.cipher.key);
        assert_eq!(by_owner, setup.cipher.key);
    }

    #[test]
    fn the_validation_hash_of_u_matches_the_password_and_no_other() {
        let setup = setup("user", "owner", -4, &entropy(4)).unwrap();
        let salt = &setup.entries.u[32..40];
        assert_eq!(&setup.entries.u[..32], &hash_r6(b"user", salt, &[]));
        assert_ne!(&setup.entries.u[..32], &hash_r6(b"other", salt, &[]));
    }

    #[test]
    fn an_empty_owner_password_falls_back_to_the_user_password() {
        let setup = setup("user", "", -4, &entropy(5)).unwrap();
        let salt = &setup.entries.o[32..40];
        assert_eq!(&setup.entries.o[..32], &hash_r6(b"user", salt, &setup.entries.u));
    }

    #[test]
    fn perms_decrypts_to_p_and_the_marker_bytes() {
        let key = [9u8; 32];
        let sealed = seal_perms(&key, -3904, [1, 2, 3, 4]);
        let mut block = GenericArray::from(sealed);
        cbc::cipher::BlockDecrypt::decrypt_block(&aes::Aes256::new((&key).into()), &mut block);
        assert_eq!(&block[..4], &(-3904i32).to_le_bytes());
        assert_eq!(&block[4..8], &[0xFF; 4]);
        assert_eq!(&block[8..12], b"Tadb");
        assert_eq!(&block[12..], &[1, 2, 3, 4]);
    }

    #[test]
    fn a_sealed_string_is_an_iv_and_the_padded_ciphertext() {
        let mut setup = setup("", "owner", -4, &entropy(6)).unwrap();
        assert_eq!(setup.cipher.seal(b"").len(), 16 + 16);
        assert_eq!(setup.cipher.seal(&[0u8; 15]).len(), 16 + 16);
        assert_eq!(setup.cipher.seal(&[0u8; 16]).len(), 16 + 32);
    }

    #[test]
    fn a_sealed_string_opens_with_the_file_key() {
        let mut setup = setup("", "owner", -4, &entropy(7)).unwrap();
        let sealed = setup.cipher.seal(b"Hello, PDF!");
        let (iv, body) = sealed.split_at(16);
        let decryptor = Aes256CbcDecryptor::new((&setup.cipher.key).into(), iv.into());
        let plain = decryptor.decrypt_padded_vec_mut::<Pkcs7>(body).unwrap();
        assert_eq!(plain, b"Hello, PDF!");
    }

    #[test]
    fn every_sealed_string_gets_its_own_iv() {
        let mut setup = setup("", "owner", -4, &entropy(8)).unwrap();
        let first  = setup.cipher.seal(b"same");
        let second = setup.cipher.seal(b"same");
        assert_ne!(first[..16], second[..16]);
        assert_ne!(first, second);
    }

    #[test]
    fn passwords_are_cut_at_127_bytes() {
        let long = "a".repeat(200);
        assert_eq!(password_bytes(&long).len(), 127);
        assert_eq!(password_bytes("short"), b"short");
    }
}
