// A document whose `<output>` asks for encryption, through the binary: the command line takes no passwords, so it
// renders one only with --skip-encryption, unencrypted, and says so without it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A folder of this test's own, in the system's temporary folder.
fn workspace() -> PathBuf {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!("lpdf-cli-encryption-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&folder).expect("a folder for the test");
    folder
}

/// A document that asks for AES-256 in its `<output>`, written to `name` in `folder`.
fn encrypted_document(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(
        &path,
        r#"<lpdf version="1"><document><output encrypt="aes-256" allow="print"/><section><layout><text>Hello</text></layout></section></document></lpdf>"#,
    )
    .expect("a file for the test");
    path
}

fn lpdf(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lpdf")).args(args).output().expect("the lpdf binary runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn is_encrypted(pdf: &[u8]) -> bool {
    pdf.windows(8).any(|window| window == b"/Encrypt")
}

#[test]
fn converting_a_document_that_asks_for_encryption_names_skip_encryption() {
    let folder = workspace();
    let input = encrypted_document(&folder, "locked.xml");
    let output_path = folder.join("locked.pdf");

    let output = lpdf(&["convert", "--input", input.to_str().unwrap(), "--output", output_path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--skip-encryption"), "{}", stderr(&output));
    assert!(!output_path.exists(), "no file is written");
}

#[test]
fn skip_encryption_converts_it_unencrypted() {
    let folder = workspace();
    let input = encrypted_document(&folder, "locked.xml");
    let output_path = folder.join("locked.pdf");

    let output = lpdf(&["convert", "--input", input.to_str().unwrap(), "--output", output_path.to_str().unwrap(), "--skip-encryption"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let pdf = std::fs::read(&output_path).expect("the PDF was written");
    assert!(pdf.starts_with(b"%PDF") && !is_encrypted(&pdf));
}

#[test]
fn a_folder_skips_the_file_without_the_flag_and_converts_it_with_it() {
    let folder = workspace();
    let input = folder.join("in");
    std::fs::create_dir_all(&input).expect("an input folder");
    encrypted_document(&input, "locked.xml");
    let out = folder.join("out");

    let output = lpdf(&["convert", "--input", input.to_str().unwrap(), "--output", out.to_str().unwrap()]);
    assert!(stderr(&output).contains("Skipping") && stderr(&output).contains("--skip-encryption"), "{}", stderr(&output));
    assert!(!out.join("locked.pdf").exists());

    let output = lpdf(&["convert", "--input", input.to_str().unwrap(), "--output", out.to_str().unwrap(), "--skip-encryption"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!is_encrypted(&std::fs::read(out.join("locked.pdf")).expect("the PDF was written")));
}

#[test]
fn benchmark_takes_skip_encryption() {
    let folder = workspace();
    let input = encrypted_document(&folder, "locked.xml");

    let output = lpdf(&["benchmark", "--input", input.to_str().unwrap(), "--repeat", "1"]);
    assert!(stderr(&output).contains("--skip-encryption"), "{}", stderr(&output));

    let output = lpdf(&["benchmark", "--input", input.to_str().unwrap(), "--repeat", "1", "--skip-encryption"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("locked"), "a timing line for the file");
}
