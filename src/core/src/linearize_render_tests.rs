//! Linearized renders of whole documents, with fonts and pictures loaded as an SDK loads them: where the linearization
//! dictionary, the cross-reference sections, the hint stream and each page's objects are in the file, and the samples
//! `make check-linearization` hands to qpdf, which reads the hints back against the file. The WASI crate has its own
//! `LpdfEngine`, so these run with the core library only.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::objects::{Value, read_object, read_value};
use crate::{LpdfEngine, RenderEncryption};

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read_file(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fixture `test/fixtures/<name>.xml`.
fn fixture(name: &str) -> String {
    String::from_utf8(read_file(&test_dir().join(format!("fixtures/{name}.xml")))).expect("UTF-8")
}

/// An unlicensed engine with the fonts and pictures the fixtures declare, loaded under their names, as an SDK loads
/// what a document's `src` attributes name.
fn engine() -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    let fonts = test_dir().join("assets/fonts");
    for (name, file) in [
        ("body", "CrimsonText-Regular.ttf"), ("heading", "CrimsonText-SemiBold.ttf"), ("sans", "Montserrat-Regular.ttf"),
    ] {
        engine.load_font(name, &read_file(&fonts.join(file)));
    }
    let images = test_dir().join("assets/images");
    for (name, file) in [("photo", "chapter-05-caterpillar.jpg"), ("chart", "image-sample.png"), ("badge", "image-square.png")] {
        engine.load_image(name, &read_file(&images.join(file)));
    }
    engine
}

fn render(xml: &str, encryption: &RenderEncryption) -> Vec<u8> {
    engine().render_xml(xml, None, encryption).unwrap_or_else(|e| panic!("the render failed: {e}"))
}

/// The passwords of the encrypted fixture, and fixed entropy, so that AES-256 gives the same file every run.
fn passwords() -> RenderEncryption {
    RenderEncryption {
        user_password:  Some("user".to_string()),
        owner_password: Some("owner".to_string()),
        entropy:        (0u8..48).collect(),
        ..Default::default()
    }
}

/// The fixture `name` rendered as an SDK renders it, unlicensed: for the snapshot tests. `showcase-linearized-encrypted`
/// is rendered with [`passwords`], in the cipher its `<output>` asks for or, with `rc4`, in RC4-128.
pub(crate) fn render_fixture(name: &str, rc4: bool) -> Vec<u8> {
    let mut xml = fixture(name);
    if rc4 {
        xml = xml.replace(r#"encrypt="aes-256""#, r#"encrypt="rc4-128""#);
    }
    let encryption = if xml.contains("encrypt=") { passwords() } else { RenderEncryption::default() };
    render(&xml, &encryption)
}

/// The same document, not linearized.
fn not_linearized(xml: &str) -> String {
    xml.replace(r#"linearize="true""#, r#"linearize="false""#)
}

/// What a linearized file says about itself: its linearization dictionary, and where each object is, from both of its
/// cross-reference sections.
struct Linearized {
    pdf:        Vec<u8>,
    dictionary: Value,
    /// Where the first page's cross-reference section is, and the main one.
    first_xref: usize,
    main_xref:  usize,
    trailer:    Value,
    offsets:    std::collections::BTreeMap<u32, usize>,
}

impl Linearized {
    fn read(pdf: Vec<u8>) -> Self {
        let open = find(&pdf, b"<< /Linearized", 0).expect("a linearization dictionary");
        let dictionary = read_value(&pdf, open).expect("the linearization dictionary reads");
        let first_xref = find(&pdf, b"xref\n", 0).expect("a cross-reference section");
        let trailer_at = find(&pdf, b"trailer", first_xref).expect("a trailer") + "trailer".len();
        let trailer = read_value(&pdf, trailer_at).expect("the trailer reads");
        let main_xref = integer(trailer.get(b"Prev"));
        let mut offsets = std::collections::BTreeMap::new();
        for at in [first_xref, main_xref] {
            offsets.extend(xref_entries(&String::from_utf8_lossy(&pdf[at..find(&pdf, b"trailer", at).unwrap()])));
        }
        Self { pdf, dictionary, first_xref, main_xref, trailer, offsets }
    }

    fn value(&self, key: &[u8]) -> usize {
        integer(self.dictionary.get(key))
    }

    fn object(&self, number: u32) -> Value {
        read_object(&self.pdf, self.offsets[&number]).expect("the object reads").value
    }

    /// The page objects, in order.
    fn pages(&self) -> Vec<u32> {
        let root = self.object(self.trailer.reference(b"Root").expect("/Root"));
        let tree = self.object(root.reference(b"Pages").expect("/Pages"));
        let Some(Value::Array(kids)) = tree.get(b"Kids") else { panic!("the page tree has no /Kids") };
        kids.iter().map(|kid| match kid { Value::Ref(number) => *number, other => panic!("a kid {other:?}") }).collect()
    }

    /// The objects page `page` reaches, itself included, other than through its `/Parent` or into another page.
    fn reached_by(&self, page: u32) -> BTreeSet<u32> {
        let mut reached = BTreeSet::from([page]);
        let mut pending = vec![self.object(page)];
        while let Some(value) = pending.pop() {
            match value {
                Value::Dict(entries) => pending.extend(entries.into_iter().filter(|(key, _)| key != b"Parent").map(|(_, v)| v)),
                Value::Array(items) => pending.extend(items),
                Value::Ref(number) => {
                    let object = self.object(number);
                    if !object.is_type(b"Page") && reached.insert(number) {
                        pending.push(object);
                    }
                }
                _ => {}
            }
        }
        reached
    }

    /// The names of a page's resources of `kind`, such as `XObject`.
    fn resource_names(&self, page: u32, kind: &[u8]) -> Vec<String> {
        match self.object(page).get(b"Resources").and_then(|resources| resources.get(kind)) {
            Some(Value::Dict(entries)) => entries.iter().map(|(name, _)| String::from_utf8_lossy(name).into_owned()).collect(),
            _ => Vec::new(),
        }
    }

    /// The object of a page's resource `name` of `kind`.
    fn resource(&self, page: u32, kind: &[u8], name: &[u8]) -> u32 {
        self.object(page).get(b"Resources").and_then(|r| r.get(kind)).and_then(|r| r.reference(name))
            .unwrap_or_else(|| panic!("page object {page} has no {}", String::from_utf8_lossy(name)))
    }
}

/// Where `needle` first is in `pdf`, from `from` on.
fn find(pdf: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    pdf[from..].windows(needle.len()).position(|window| window == needle).map(|at| from + at)
}

fn integer(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Integer(number)) => *number as usize,
        other => panic!("not a whole number: {other:?}"),
    }
}

/// The objects of the cross-reference section at the start of `text`, with their offsets.
fn xref_entries(text: &str) -> Vec<(u32, usize)> {
    let mut lines = text.lines().skip(1);
    let header: Vec<u32> = lines.next().unwrap().split_whitespace().map(|n| n.parse().unwrap()).collect();
    let (first, count) = (header[0], header[1]);
    (first..first + count)
        .zip(lines)
        .filter(|(_, line)| line.trim_end().ends_with('n'))
        .map(|(number, line)| (number, line[..10].parse().unwrap()))
        .collect()
}

/// The data of the hint stream, which is not encrypted in a file that isn't, and where its shared object table starts.
fn hint_data(file: &Linearized) -> (Vec<u8>, usize) {
    let Some(Value::Array(h)) = file.dictionary.get(b"H") else { panic!("no /H") };
    let offset = integer(h.first());
    let object = read_object(&file.pdf, offset).expect("the hint stream reads");
    let range = object.parts.iter().find(|(_, part)| *part == crate::objects::Part::StreamData).expect("stream data").0.clone();
    (file.pdf[range].to_vec(), integer(object.value.get(b"S")))
}

fn word(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().unwrap())
}

#[test]
fn the_file_starts_with_the_linearization_dictionary_and_the_first_pages_cross_references() {
    for name in ["showcase-linearized", "showcase-linearized-pages"] {
        let file = Linearized::read(render_fixture(name, false));
        let first_object = file.offsets.values().min().copied().unwrap();
        assert!(file.pdf[first_object..].starts_with(format!("{} 0 obj\n<< /Linearized 1 ", file.offsets.iter().find(|(_, at)| **at == first_object).unwrap().0).as_bytes()),
            "{name}: the dictionary is the first object");
        assert!(file.pdf[..first_object].starts_with(b"%PDF-"), "{name}: only the header before it");
        let text = String::from_utf8_lossy(&file.pdf[file.main_xref..]).into_owned();
        assert_eq!(file.value(b"L"), file.pdf.len(), "{name}: /L is the file's length");
        assert_eq!(file.value(b"N"), file.pages().len(), "{name}: /N counts the pages");
        assert_eq!(file.value(b"O") as u32, file.pages()[0], "{name}: /O is page 1");
        assert!(file.first_xref < file.offsets[&file.pages()[0]], "{name}: the first section's table is before page 1");
        assert!(file.trailer.get(b"Root").is_some() && file.trailer.get(b"Info").is_some(), "{name}: the first trailer");
        assert!(text.ends_with(&format!("startxref\n{}\n%%EOF\n", file.first_xref)), "{name}: the last startxref");
        let t = file.value(b"T");
        assert!(file.main_xref < t && String::from_utf8_lossy(&file.pdf[t..t + 30]).trim_start().starts_with("0000000000 65535 f"), "{name}: /T");
    }
}

#[test]
fn page_1_and_everything_it_draws_end_where_e_says() {
    for name in ["showcase-linearized", "showcase-linearized-pages"] {
        let file = Linearized::read(render_fixture(name, false));
        let pages = file.pages();
        let Some(Value::Array(h)) = file.dictionary.get(b"H") else { panic!("no /H") };
        let hint_offset = integer(h.first());
        let end = file.value(b"E");
        for number in file.reached_by(pages[0]) {
            let at = file.offsets[&number];
            assert!(hint_offset < at && at < end, "{name}: object {number} of page 1 is at {at}, outside its section ..{end}");
        }
        for &page in &pages[1..] {
            assert!(file.offsets[&page] >= end, "{name}: page object {page} is in page 1's section");
        }
    }
}

/// The objects each page reaches that no other page does.
fn private_objects(file: &Linearized, pages: &[u32]) -> Vec<BTreeSet<u32>> {
    let reached: Vec<BTreeSet<u32>> = pages.iter().map(|&page| file.reached_by(page)).collect();
    (0..pages.len())
        .map(|i| reached[i].iter().copied().filter(|n| (0..pages.len()).all(|j| j == i || !reached[j].contains(n))).collect())
        .collect()
}

#[test]
fn the_other_pages_follow_in_page_order_each_with_the_objects_only_it_draws() {
    let file = Linearized::read(render_fixture("showcase-linearized-pages", false));
    let pages = file.pages();
    assert_eq!(pages.len(), 6);
    let starts: Vec<usize> = pages.iter().map(|page| file.offsets[page]).collect();
    assert!(starts.windows(2).all(|pair| pair[0] < pair[1]), "pages in order: {starts:?}");
    let private = private_objects(&file, &pages);
    for i in 1..pages.len() {
        let next_start = starts.get(i + 1).copied().unwrap_or(usize::MAX);
        let numbers: Vec<u32> = private[i].iter().copied().collect();
        // Numbered one after the other from the page object on, as the hints count them, and written between the page
        // and the next.
        assert_eq!(numbers, (pages[i]..pages[i] + numbers.len() as u32).collect::<Vec<_>>(), "page {}", i + 1);
        for number in numbers {
            let at = file.offsets[&number];
            assert!(starts[i] <= at && at < next_start, "page {} object {number} at {at} is out of its place", i + 1);
        }
    }
}

#[test]
fn what_later_pages_share_comes_after_the_last_page_and_what_one_page_draws_goes_with_it() {
    let file = Linearized::read(render_fixture("showcase-linearized-pages", false));
    let pages = file.pages();
    let picture = |page: u32| file.resource(page, b"XObject", file.resource_names(page, b"XObject")[0].as_bytes());
    let photo = picture(pages[1]);
    assert_eq!(photo, picture(pages[3]), "pages 2 and 4 draw the one photo");
    let private = private_objects(&file, &pages);
    let last_private = private.iter().skip(1).flatten().map(|n| file.offsets[n]).max().unwrap();
    assert!(file.offsets[&photo] > last_private, "the shared photo comes after every page's own objects");

    let chart = picture(pages[2]);
    assert!(private[2].contains(&chart), "the chart is page 3's own");
    let badge = picture(pages[0]);
    assert!(file.offsets[&badge] < file.value(b"E"), "the badge goes with page 1");
}

/// How many of the pictures `I0`, `I1` and `I2` each `/XObject << … >>` of `pdf` names: those of the pages, and of the
/// transparency groups, which name what a page does.
fn pictures_named(pdf: &[u8]) -> Vec<usize> {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/XObject <<")
        .map(|(at, key)| {
            let rest = &text[at + key.len()..];
            let dictionary = &rest[..rest.find(">>").unwrap()];
            ["/I0 ", "/I1 ", "/I2 "].iter().filter(|name| dictionary.contains(*name)).count()
        })
        .collect()
}

#[test]
fn a_page_names_only_the_pictures_and_fonts_it_draws() {
    let xml = fixture("showcase-linearized-pages");
    let file = Linearized::read(render(&xml, &RenderEncryption::default()));
    let pages = file.pages();
    let pictures = |page: u32| file.resource_names(page, b"XObject").into_iter().filter(|name| name.starts_with('I')).count();
    assert_eq!(pictures(pages[0]), 1, "page 1 draws the badge only");
    assert_eq!(pictures(pages[2]), 1, "page 3 draws the chart only");
    assert_eq!(pictures(pages[4]), 0, "page 5 draws no picture");
    assert!(file.resource_names(pages[1], b"Font").len() < file.resource_names(pages[2], b"Font").len(), "sans is on page 3, not 2");

    // Not linearized, every page and group names all three pictures, as before.
    let plain = render(&not_linearized(&xml), &RenderEncryption::default());
    let named = pictures_named(&plain);
    assert!(named.len() >= 6 && named.iter().all(|&count| count == 3), "{named:?}");
}

#[test]
fn the_hint_stream_holds_the_page_offset_and_shared_object_tables() {
    let file = Linearized::read(render_fixture("showcase-linearized-pages", false));
    let pages = file.pages();
    let (data, shared) = hint_data(&file);
    let Some(Value::Array(h)) = file.dictionary.get(b"H") else { panic!("no /H") };
    let hint_length = integer(h.get(1));
    // Item 2 of the page offset table, where page 1 is, as if the hint stream were not in the file.
    assert_eq!(word(&data, 4) as usize + hint_length, file.offsets[&pages[0]]);
    // The shared object table: page 1's section is every object page 1 draws and the outline the document opens with;
    // the section of what later pages share adds the photo and the objects of the sans font.
    let first_page_shared = word(&data, shared + 8) as usize;
    let total = word(&data, shared + 12) as usize;
    assert_eq!(first_page_shared, file.reached_by(pages[0]).len() + count_outline(&file));
    let reached: Vec<BTreeSet<u32>> = pages.iter().map(|&page| file.reached_by(page)).collect();
    let later_shared: BTreeSet<u32> = reached[1..].iter().flatten().copied()
        .filter(|n| !reached[0].contains(n) && reached[1..].iter().filter(|set| set.contains(n)).count() > 1)
        .collect();
    assert_eq!(total, first_page_shared + later_shared.len());
    let first_shared = later_shared.iter().min_by_key(|n| file.offsets[*n]).unwrap();
    assert_eq!(word(&data, shared), *first_shared, "item 1: the first of the shared section");
    assert_eq!(word(&data, shared + 4) as usize + hint_length, file.offsets[first_shared], "item 2: where it is");
}

/// The objects of the outline: in page 1's section, since the document opens with it showing.
fn count_outline(file: &Linearized) -> usize {
    let root = file.object(file.trailer.reference(b"Root").unwrap());
    let Some(outline) = root.reference(b"Outlines") else { return 0 };
    let mut reached = BTreeSet::from([outline]);
    let mut pending = vec![file.object(outline)];
    while let Some(value) = pending.pop() {
        match value {
            Value::Dict(entries) => pending.extend(entries.into_iter().map(|(_, v)| v)),
            Value::Array(items) => pending.extend(items),
            Value::Ref(number) => {
                let object = file.object(number);
                if !object.is_type(b"Page") && reached.insert(number) {
                    pending.push(object);
                }
            }
            _ => {}
        }
    }
    reached.len()
}

#[test]
fn linearize_false_writes_the_file_as_without_the_attribute() {
    let xml = fixture("showcase-linearized");
    let without = xml.replace(r#"<output linearize="true" />"#, "");
    assert_ne!(without, xml);
    assert_eq!(render(&not_linearized(&xml), &RenderEncryption::default()), render(&without, &RenderEncryption::default()));
}

#[test]
fn the_same_document_gives_the_same_bytes() {
    for (name, rc4) in [("showcase-linearized-pages", false), ("showcase-linearized-encrypted", false), ("showcase-linearized-encrypted", true)] {
        assert_eq!(render_fixture(name, rc4), render_fixture(name, rc4), "{name}");
    }
}

#[test]
fn an_encrypted_file_is_encrypted_first_and_linearized_last() {
    for rc4 in [false, true] {
        let file = Linearized::read(render_fixture("showcase-linearized-encrypted", rc4));
        assert_eq!(file.value(b"L"), file.pdf.len());
        let encrypt = file.trailer.reference(b"Encrypt").expect("the first trailer names /Encrypt");
        let dictionary = file.object(encrypt);
        let revision = integer(dictionary.get(b"R"));
        assert_eq!(revision, if rc4 { 3 } else { 6 });
        assert!(file.offsets[&encrypt] < file.offsets[&file.pages()[0]], "the encryption dictionary comes before page 1");
        let (data, _) = hint_data(&file);
        if !rc4 {
            assert_eq!(data.len() % 16, 0, "the hint stream is encrypted with AES-256: an IV and whole blocks");
        }
    }
}

#[test]
fn the_kit_tree_asks_for_it_with_a_boolean() {
    let tree = |value: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{{"output":{{"linearize":{value}}}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["a"]}}]}}]}}]}}"#
    );
    let pdf = LpdfEngine::new("").render_json(&tree("true"), &RenderEncryption::default()).expect("a render");
    assert!(String::from_utf8_lossy(&pdf).contains("/Linearized 1"));
    let pdf = LpdfEngine::new("").render_json(&tree("false"), &RenderEncryption::default()).expect("a render");
    assert!(!String::from_utf8_lossy(&pdf).contains("/Linearized"));
    let error = LpdfEngine::new("").render_json(&tree(r#""yes""#), &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("linearize must be true or false"), "{error}");
}

#[test]
fn an_xml_value_other_than_true_or_false_is_refused() {
    let xml = r#"<lpdf version="1"><document><output linearize="yes"/><section><layout><text>a</text></layout></section></document></lpdf>"#;
    let error = LpdfEngine::new("").render_xml(xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("linearize"), "{error}");
}

/// The book example, all twelve chapters, with its fonts and plates loaded from its folder.
fn render_book(linearize: bool) -> Vec<u8> {
    let folder = test_dir().join("../examples/6-book");
    let mut xml = String::from_utf8(read_file(&folder.join("document.xml"))).expect("UTF-8");
    if linearize {
        let at = xml.find("<document").and_then(|at| xml[at..].find('>').map(|end| at + end + 1)).expect("<document>");
        xml.insert_str(at, r#"<output linearize="true"/>"#);
    }
    let mut engine = LpdfEngine::new("");
    for (name, file) in [("body", "CrimsonText-Regular.ttf"), ("italic", "CrimsonText-Italic.ttf"), ("heading", "CrimsonText-SemiBold.ttf")] {
        engine.load_font(name, &read_file(&folder.join("assets/fonts").join(file)));
    }
    for file in crate::source_files(&xml).expect("the book's files") {
        if file.kind == crate::SourceKind::Image {
            engine.load_image(&file.key, &read_file(&folder.join(&file.src)));
        }
    }
    engine.render_xml(&xml, None, &RenderEncryption::default()).expect("the book renders")
}

/// Writes each linearized sample with the same document not linearized, for `make check-linearization`
/// (`scripts/check-linearization.mjs`): the fixtures, the encrypted one in both ciphers, the forms and PDF/UA
/// fixtures, whose objects go to other sections, and the book example.
#[test]
fn writes_linearized_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_LINEARIZE_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    let write = |name: &str, bytes: Vec<u8>| std::fs::write(dir.join(name), bytes).expect("a sample");

    for (sample, name) in [("one-page", "showcase-linearized"), ("pages", "showcase-linearized-pages")] {
        let xml = fixture(name);
        write(&format!("{sample}.pdf"), render(&xml, &RenderEncryption::default()));
        write(&format!("{sample}-plain.pdf"), render(&not_linearized(&xml), &RenderEncryption::default()));
    }

    let encrypted = fixture("showcase-linearized-encrypted");
    write("encrypted-aes256.pdf", render_fixture("showcase-linearized-encrypted", false));
    write("encrypted-rc4.pdf", render_fixture("showcase-linearized-encrypted", true));
    write("encrypted-plain.pdf", render(&not_linearized(&encrypted), &RenderEncryption::skipping(true)));

    let forms = fixture("showcase-forms");
    let linearized_forms = forms.replacen(r#"<document size="a4">"#, r#"<document size="a4"><output linearize="true"/>"#, 1);
    assert_ne!(forms, linearized_forms);
    write("forms.pdf", render(&linearized_forms, &RenderEncryption::default()));
    write("forms-plain.pdf", render(&forms, &RenderEncryption::default()));

    let pdfua = fixture("showcase-pdfua");
    let linearized_pdfua = pdfua.replacen(r#"<output pdfua="1" />"#, r#"<output pdfua="1" linearize="true" />"#, 1);
    assert_ne!(pdfua, linearized_pdfua);
    write("pdfua.pdf", crate::pdfua_render_tests::render(&linearized_pdfua).expect("PDF/UA renders"));
    write("pdfua-plain.pdf", crate::pdfua_render_tests::render(&pdfua).expect("PDF/UA renders"));

    write("book.pdf", render_book(true));
    write("book-plain.pdf", render_book(false));
}
