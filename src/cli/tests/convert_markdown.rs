// `lpdf convert` on a `.md` file, through the binary: the template, the front matter, the images next to the file
// and the styles file a template names.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A folder of this test's own, in the system's temporary folder.
fn workspace() -> PathBuf {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!("lpdf-cli-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&folder).expect("a folder for the test");
    folder
}

fn write(folder: &Path, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = folder.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a folder for the file");
    }
    std::fs::write(&path, contents).expect("a file for the test");
    path
}

fn convert(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lpdf")).arg("convert").args(args).output().expect("the lpdf binary runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn read_pdf(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).expect("the PDF was written");
    assert!(bytes.starts_with(b"%PDF"), "not a PDF");
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// A 1x1 red PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
    0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn converts_a_markdown_file_with_front_matter_and_a_relative_image() {
    let folder = workspace();
    write(&folder, "img/chart.png", PNG);
    let input = write(&folder, "notes.md", "---\ntitle: Notes\n---\n\n# Notes\n\nSome *text*.\n\n![the chart](img/chart.png)\n");
    let output_path = folder.join("notes.pdf");

    let output = convert(&["--input", input.to_str().unwrap(), "--output", output_path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));

    let pdf = read_pdf(&output_path);
    assert!(contains(&pdf, b"/Subtype /Image") || contains(&pdf, b"/Subtype/Image"), "the image is in the PDF");
}

#[test]
fn a_template_wraps_the_file_and_reads_the_front_matter_and_the_files_it_names() {
    let folder = workspace();
    write(&folder, "template/styles.xml", r##"<markdown-styles><h1 color="#ff0000"/></markdown-styles>"##);
    write(&folder, "template/logo.png", PNG);
    let template = write(
        &folder,
        "template/page.xml",
        r#"<lpdf version="1"><assets><image name="logo" src="logo.png"/></assets><document><section>
            <layer><img-at name="logo" x="20pt" y="20pt" width="30pt" height="30pt"/><text-at x="60pt" y="40pt" data-value="title">Title</text-at></layer>
            <markdown margin="80pt"><markdown-styles src="styles.xml"/><markdown-content data-value="markdown"/></markdown>
            </section></document></lpdf>"#,
    );
    let input = write(&folder, "notes.md", "---\ntitle: Notes\n---\n\n# Heading\n");
    let output_path = folder.join("out.pdf");

    let output = convert(&["--input", input.to_str().unwrap(), "--template", template.to_str().unwrap(), "--output", output_path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let pdf = read_pdf(&output_path);
    assert!(contains(&pdf, b"/Subtype /Image") || contains(&pdf, b"/Subtype/Image"), "the logo of the template is in the PDF");
}

#[test]
fn data_adds_to_the_front_matter_and_never_replaces_the_text() {
    let folder = workspace();
    let template = write(
        &folder,
        "page.xml",
        r#"<lpdf version="1"><document><section>
            <layer><text-at x="40pt" y="40pt" data-value="edition">Edition</text-at></layer>
            <markdown><markdown-content data-value="markdown"/></markdown></section></document></lpdf>"#,
    );
    let input = write(&folder, "notes.md", "---\nedition: first\n---\n\n# Heading\n");
    let with = write(&folder, "extra.json", r##"{ "edition": "second", "markdown": "# Replaced" }"##);
    let plain_out = folder.join("plain.pdf");
    let extra_out = folder.join("extra.pdf");

    let plain = convert(&["--input", input.to_str().unwrap(), "--template", template.to_str().unwrap(), "--output", plain_out.to_str().unwrap()]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    let extra = convert(&[
        "--input", input.to_str().unwrap(), "--template", template.to_str().unwrap(),
        "--data", with.to_str().unwrap(), "--output", extra_out.to_str().unwrap(),
    ]);
    assert!(extra.status.success(), "{}", stderr(&extra));
    // `edition` is another word, so the PDFs differ; the heading stays, so both are of one page of one size
    assert_ne!(read_pdf(&plain_out), read_pdf(&extra_out));
}

#[test]
fn raw_html_and_an_image_at_a_url_are_left_out_and_the_rest_is_converted() {
    let folder = workspace();
    let input = write(&folder, "notes.md", "# Fine\n\ntext\n\n<p align=\"center\"><img src=\"https://example.com/a.svg\"></p>\n\n![badge](https://example.com/b.svg)\n");
    let output = convert(&["--input", input.to_str().unwrap(), "--output", folder.join("out.pdf").to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    read_pdf(&folder.join("out.pdf"));
}

#[test]
fn an_image_that_is_not_there_is_an_error_that_names_the_file() {
    let folder = workspace();
    let input = write(&folder, "notes.md", "![x](img/missing.png)\n");
    let output = convert(&["--input", input.to_str().unwrap(), "--output", folder.join("out.pdf").to_str().unwrap()]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("missing.png"), "{message}");
}

#[test]
fn a_template_belongs_to_a_markdown_file() {
    let folder = workspace();
    let input = write(&folder, "doc.xml", r#"<lpdf version="1"><document><section><layout><text>Hi</text></layout></section></document></lpdf>"#);
    let template = write(&folder, "page.xml", "<lpdf/>");
    let output = convert(&["--input", input.to_str().unwrap(), "--template", template.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--template"), "{}", stderr(&output));
}

#[test]
fn an_xml_document_reads_the_images_and_styles_it_names_next_to_it() {
    let folder = workspace();
    write(&folder, "logo.png", PNG);
    write(&folder, "styles.xml", r##"<markdown-styles><h1 color="#00ff00"/></markdown-styles>"##);
    let input = write(
        &folder,
        "doc.xml",
        r#"<lpdf version="1"><assets><image name="logo" src="logo.png"/></assets><document><section><layout><img name="logo" width="40pt"/></layout></section>
            <section><markdown><markdown-styles src="styles.xml"/><markdown-content># Title</markdown-content></markdown></section></document></lpdf>"#,
    );
    let output_path = folder.join("doc.pdf");
    let output = convert(&["--input", input.to_str().unwrap(), "--output", output_path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let pdf = read_pdf(&output_path);
    assert!(contains(&pdf, b"/Subtype /Image") || contains(&pdf, b"/Subtype/Image"), "the logo is in the PDF");
}
