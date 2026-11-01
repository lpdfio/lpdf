//! PDF/X (ISO 15930), which `<output pdfx>` asks for: the version, and the press profile of the output intent, which
//! the document declares as a `<profile>` of `<assets>` and `<output profile>` names. The engine bundles no press
//! profile: the printer gives out its own. What a PDF/X file refuses is checked in `pdf.rs`, where the fonts, the
//! colours and the fields are known, and the XMP packet is written with PDF/A's and PDF/UA's (`pdfa::xmp_packet`).

use std::collections::HashMap;

use crate::parse::Meta;

/// A version of PDF/X, as `<output pdfx>` writes it. PDF/X-4 keeps RGB colours and transparency, tied to ICC
/// profiles; PDF/X-1a, which allows neither, is not written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfXVersion {
    X4,
}

impl PdfXVersion {
    /// The version of a `pdfx` attribute: `4`.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "4" => Ok(PdfXVersion::X4),
            other => Err(format!("pdfx '{other}' is not a PDF/X version the engine writes: use 4")),
        }
    }

    /// The value of the `pdfx` attribute, for messages: `4`.
    pub fn attribute(self) -> &'static str {
        match self {
            PdfXVersion::X4 => "4",
        }
    }

    /// The version as `GTS_PDFXVersion` in the Info dictionary and `pdfxid:GTS_PDFXVersion` in the XMP packet write it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            PdfXVersion::X4 => "PDF/X-4",
        }
    }
}

/// What `<output>` says about PDF/X: the version of `pdfx`, and the `<profile>` asset `profile` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfX {
    pub version: PdfXVersion,
    pub profile: String,
}

/// The `<output>` attributes `pdfx` and `profile`, which go together: `None` when neither is there.
pub(crate) fn parse(pdfx: Option<&str>, profile: Option<&str>) -> Result<Option<PdfX>, String> {
    let version = pdfx.map(PdfXVersion::parse).transpose()?;
    match (version, profile) {
        (None, None) => Ok(None),
        (Some(version), Some(profile)) if !profile.is_empty() => Ok(Some(PdfX { version, profile: profile.to_string() })),
        (Some(version), _) => Err(format!(
            "pdfx=\"{}\" needs profile, the <profile> of <assets> the file is made for: the ICC profile of the printing \
            press, which the printer gives out. Declare it in <assets>, as <profile name=\"press\" src=\"press.icc\"/>, \
            and add profile=\"press\"",
            version.attribute()
        )),
        (None, Some(_)) => Err("profile names the press profile of a PDF/X file, and the document is not one: add \
            pdfx=\"4\", or remove profile".to_string()),
    }
}

/// The bytes of the colour profiles documents name, by the key they were loaded under: a `<profile>`'s `ref`, or with
/// no ref its name.
#[derive(Debug, Clone, Default)]
pub struct ProfileRegistry {
    profiles: HashMap<String, Vec<u8>>,
}

impl ProfileRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self, key: &str, bytes: Vec<u8>) {
        self.profiles.insert(key.to_string(), bytes);
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.profiles.get(key).map(Vec::as_slice)
    }
}

/// The colour space an output intent's profile prints in, from its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileSpace {
    Gray,
    Rgb,
    Cmyk,
}

impl ProfileSpace {
    /// The number of components, the `/N` of the profile's stream.
    pub(crate) fn components(self) -> i32 {
        match self {
            ProfileSpace::Gray => 1,
            ProfileSpace::Rgb => 3,
            ProfileSpace::Cmyk => 4,
        }
    }

    /// What it is called in a message: `a gray`, `an RGB`.
    pub(crate) fn article(self) -> &'static str {
        match self {
            ProfileSpace::Gray => "a gray",
            ProfileSpace::Rgb => "an RGB",
            ProfileSpace::Cmyk => "a CMYK",
        }
    }
}

/// The press profile of a PDF/X file's output intent, read and checked: its bytes, the colour space it prints in, and
/// the description it gives itself, which the output intent's `/Info` repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PressProfile<'a> {
    /// The name of its `<profile>`, for messages.
    pub name:        String,
    pub bytes:       &'a [u8],
    pub space:       ProfileSpace,
    pub description: String,
}

/// The output intent of a document whose `<output>` asks for PDF/X: the `<profile>` asset `profile` names, read from
/// `registry`, and checked to be a printer profile. A PDF/X file also names its title, so a document without one is
/// refused here, before the layout. `None` for a document that doesn't ask for PDF/X.
pub(crate) fn resolve<'a>(
    pdfx:     Option<&PdfX>,
    profiles: &HashMap<String, String>,
    registry: &'a ProfileRegistry,
    meta:     &Meta,
) -> Result<Option<PressProfile<'a>>, String> {
    let Some(pdfx) = pdfx else { return Ok(None) };
    let refuse = |reason: String| Err(format!("<output pdfx=\"{}\">: {reason}", pdfx.version.attribute()));
    if meta.title.trim().is_empty() {
        return refuse("a PDF/X file names its title, which the printer's tools show: add title to <meta>".to_string());
    }
    let name = &pdfx.profile;
    let Some(key) = profiles.get(name) else {
        return refuse(format!(
            "profile '{name}' is not a <profile> of <assets>: declare it there, as <profile name=\"{name}\" \
            src=\"{name}.icc\"/>"
        ));
    };
    let Some(bytes) = registry.get(key) else {
        return refuse(format!(
            "the profile '{name}' of <assets> was not loaded: give it a src the SDK reads the ICC file from, or load it \
            on the engine with loadProfile('{key}', bytes)"
        ));
    };
    read_profile(name, bytes).map(Some).or_else(refuse)
}

/// Read the header of the ICC profile `bytes`, which `<profile name>` declares, and check that it can be a PDF/X
/// output intent: an ICC profile of the printer (output) class, in gray, RGB or CMYK (ICC.1:2010, 7.2).
pub(crate) fn read_profile<'a>(name: &str, bytes: &'a [u8]) -> Result<PressProfile<'a>, String> {
    if bytes.len() < 132 || bytes.get(36..40) != Some(b"acsp") || be_u32(bytes, 0) != Some(bytes.len()) {
        return Err(format!("the profile '{name}' is not an ICC profile: give the .icc file of the printer's profile"));
    }
    let class = &bytes[12..16];
    if class != b"prtr" {
        let kind = match class {
            b"mntr" => "a display profile, such as sRGB",
            b"scnr" => "an input profile, of a scanner or a camera",
            b"link" => "a device link",
            b"spac" => "a color space profile",
            b"abst" => "an abstract profile",
            b"nmcl" => "a named color profile",
            _ => "not of a class PDF/X knows",
        };
        return Err(format!(
            "the profile '{name}' is {kind}, not a printer profile: the output intent of a PDF/X file is the profile of \
            the printing press, such as a FOGRA39 or GRACoL one, which the printer gives out"
        ));
    }
    let space = match &bytes[16..20] {
        b"GRAY" => ProfileSpace::Gray,
        b"RGB " => ProfileSpace::Rgb,
        b"CMYK" => ProfileSpace::Cmyk,
        other => {
            return Err(format!(
                "the profile '{name}' prints in the color space '{}', and a PDF/X output intent is a gray, RGB or CMYK \
                one: ask the printer for a CMYK profile of the press",
                String::from_utf8_lossy(other).trim()
            ));
        }
    };
    let description = description(bytes).filter(|d| !d.is_empty()).unwrap_or_else(|| name.to_string());
    Ok(PressProfile { name: name.to_string(), bytes, space, description })
}

/// The text of the profile's `desc` tag: the ASCII of a version 2 `textDescriptionType`, or the first record of a
/// version 4 `multiLocalizedUnicodeType`. `None` when there is none, or it can't be read.
fn description(bytes: &[u8]) -> Option<String> {
    let u32_at = |at: usize| be_u32(bytes, at);
    let count = u32_at(128)?;
    let (offset, size) = (0..count.min(256)).find_map(|i| {
        let entry = 132 + 12 * i;
        (bytes.get(entry..entry + 4)? == b"desc").then(|| Some((u32_at(entry + 4)?, u32_at(entry + 8)?)))?
    })?;
    let tag = bytes.get(offset..offset.checked_add(size)?)?;
    let text = match tag.get(0..4)? {
        b"desc" => {
            let length = u32_at(offset + 8)?;
            let ascii = tag.get(12..12 + length)?;
            String::from_utf8_lossy(ascii.split(|&b| b == 0).next()?).into_owned()
        }
        b"mluc" => {
            let records = u32_at(offset + 8)?;
            if records == 0 {
                return None;
            }
            let length = u32_at(offset + 20)?;
            let start = u32_at(offset + 24)?;
            let units: Vec<u16> = tag.get(start..start + length)?
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => return None,
    };
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    Some(text.trim().to_string())
}

/// The big-endian 32-bit number at `at` of `bytes`, as ICC writes its sizes and offsets.
fn be_u32(bytes: &[u8], at: usize) -> Option<usize> {
    let &[a, b, c, d] = bytes.get(at..at.checked_add(4)?)? else { return None };
    Some(u32::from_be_bytes([a, b, c, d]) as usize)
}

/// The header of an ICC profile of `class` in `space`, with a `desc` tag of version 2 that says `text`, and nothing
/// else: enough for `read_profile`, for tests.
#[cfg(test)]
pub(crate) fn test_profile(class: &[u8; 4], space: &[u8; 4], text: &str) -> Vec<u8> {
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend_from_slice(&(text.len() as u32 + 1).to_be_bytes());
    desc.extend_from_slice(text.as_bytes());
    desc.push(0);
    desc.extend_from_slice(&[0; 12]);
    let mut bytes = vec![0u8; 128];
    bytes[12..16].copy_from_slice(class);
    bytes[16..20].copy_from_slice(space);
    bytes[20..24].copy_from_slice(b"Lab ");
    bytes[36..40].copy_from_slice(b"acsp");
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(b"desc");
    bytes.extend_from_slice(&144u32.to_be_bytes());
    bytes.extend_from_slice(&(desc.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&desc);
    let size = bytes.len() as u32;
    bytes[0..4].copy_from_slice(&size.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(class: &[u8; 4], space: &[u8; 4], text: &str) -> Vec<u8> {
        test_profile(class, space, text)
    }

    #[test]
    fn a_version_is_read_from_its_attribute_value() {
        assert_eq!(PdfXVersion::parse("4"), Ok(PdfXVersion::X4));
        for other in ["1a", "3", "4p", "X-4", ""] {
            let error = PdfXVersion::parse(other).unwrap_err();
            assert!(error.contains("use 4"), "{error}");
        }
    }

    #[test]
    fn pdfx_and_profile_go_together() {
        assert_eq!(parse(None, None), Ok(None));
        assert_eq!(
            parse(Some("4"), Some("press")),
            Ok(Some(PdfX { version: PdfXVersion::X4, profile: "press".into() }))
        );
        let no_profile = parse(Some("4"), None).unwrap_err();
        assert!(no_profile.starts_with("pdfx=\"4\" needs profile") && no_profile.contains("<profile name="), "{no_profile}");
        assert!(parse(Some("4"), Some("")).unwrap_err().contains("needs profile"));
        let no_pdfx = parse(None, Some("press")).unwrap_err();
        assert!(no_pdfx.contains("add pdfx=\"4\", or remove profile"), "{no_pdfx}");
    }

    #[test]
    fn a_printer_profile_is_read_with_its_space_and_description() {
        let bytes = profile(b"prtr", b"CMYK", "FOGRA39L Coated");
        let press = read_profile("press", &bytes).unwrap();
        assert_eq!(press.space, ProfileSpace::Cmyk);
        assert_eq!(press.space.components(), 4);
        assert_eq!(press.description, "FOGRA39L Coated");
        for (space, expected) in [(b"RGB ", ProfileSpace::Rgb), (b"GRAY", ProfileSpace::Gray)] {
            assert_eq!(read_profile("p", &profile(b"prtr", space, "x")).unwrap().space, expected);
        }
    }

    #[test]
    fn a_profile_with_no_description_is_described_by_its_name() {
        let mut bytes = profile(b"prtr", b"CMYK", "x");
        bytes[128..132].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(read_profile("press", &bytes).unwrap().description, "press");
    }

    #[test]
    fn a_profile_that_is_not_a_printer_profile_is_an_error_that_says_what_it_is() {
        let srgb = read_profile("screen", crate::pdfa::SRGB_PROFILE).unwrap_err();
        assert!(srgb.contains("'screen' is a display profile, such as sRGB, not a printer profile"), "{srgb}");
        let input = read_profile("cam", &profile(b"scnr", b"RGB ", "x")).unwrap_err();
        assert!(input.contains("an input profile"), "{input}");
        let lab = read_profile("lab", &profile(b"prtr", b"Lab ", "x")).unwrap_err();
        assert!(lab.contains("color space 'Lab'") && lab.contains("CMYK profile"), "{lab}");
    }

    #[test]
    fn bytes_that_are_not_an_icc_profile_are_an_error() {
        for bytes in [b"not a profile".to_vec(), vec![0u8; 200], {
            let mut b = profile(b"prtr", b"CMYK", "x");
            b[0..4].copy_from_slice(&999u32.to_be_bytes());
            b
        }] {
            let error = read_profile("press", &bytes).unwrap_err();
            assert!(error.contains("'press' is not an ICC profile"), "{error}");
        }
    }

    #[test]
    fn resolving_needs_a_title_a_declared_profile_and_its_bytes() {
        let pdfx = PdfX { version: PdfXVersion::X4, profile: "press".into() };
        let titled = Meta { title: "Brochure".into(), ..Meta::default() };
        let declared: HashMap<String, String> = [("press".to_string(), "press-key".to_string())].into();
        let mut registry = ProfileRegistry::new();

        assert_eq!(resolve(None, &HashMap::new(), &registry, &Meta::default()), Ok(None));

        let untitled = resolve(Some(&pdfx), &declared, &registry, &Meta::default()).unwrap_err();
        assert!(untitled.starts_with("<output pdfx=\"4\">") && untitled.contains("add title to <meta>"), "{untitled}");

        let undeclared = resolve(Some(&pdfx), &HashMap::new(), &registry, &titled).unwrap_err();
        assert!(undeclared.contains("profile 'press' is not a <profile> of <assets>"), "{undeclared}");

        let unloaded = resolve(Some(&pdfx), &declared, &registry, &titled).unwrap_err();
        assert!(unloaded.contains("was not loaded") && unloaded.contains("loadProfile('press-key', bytes)"), "{unloaded}");

        registry.load("press-key", crate::pdfa::SRGB_PROFILE.to_vec());
        let display = resolve(Some(&pdfx), &declared, &registry, &titled).unwrap_err();
        assert!(display.starts_with("<output pdfx=\"4\">: the profile 'press' is a display profile"), "{display}");

        registry.load("press-key", profile(b"prtr", b"CMYK", "Press"));
        let press = resolve(Some(&pdfx), &declared, &registry, &titled).unwrap().unwrap();
        assert_eq!((press.name.as_str(), press.space), ("press", ProfileSpace::Cmyk));
    }
}
