// A document that carries files, through the binary: `convert` reads each `<file src>` relative to the document, as
// it reads an image's, and the PDF carries the file.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/showcase-attachments.xml")
}

fn lpdf(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lpdf")).args(args).output().expect("the lpdf binary runs")
}

#[test]
fn convert_reads_the_files_a_document_carries_from_their_src() {
    let output_path = std::env::temp_dir().join(format!("lpdf-cli-attachments-{}.pdf", std::process::id()));
    let output = lpdf(&["convert", "--input", fixture().to_str().unwrap(), "--output", output_path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let pdf = String::from_utf8_lossy(&std::fs::read(&output_path).expect("the PDF was written")).into_owned();
    assert_eq!(pdf.matches("/Type /EmbeddedFile").count(), 2, "both files are carried");
    std::fs::remove_file(&output_path).ok();
}

#[test]
fn convert_says_which_file_it_could_not_read() {
    let folder = std::env::temp_dir().join(format!("lpdf-cli-attachments-missing-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("a folder for the test");
    let input = folder.join("doc.xml");
    std::fs::write(
        &input,
        r#"<lpdf version="1"><document><attachments><file name="data.csv" src="data.csv"/></attachments>
            <section><layout><text>x</text></layout></section></document></lpdf>"#,
    )
    .expect("a file for the test");
    let output = lpdf(&["convert", "--input", input.to_str().unwrap(), "--output", folder.join("doc.pdf").to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to read the file") && stderr.contains("data.csv"), "{stderr}");
}
