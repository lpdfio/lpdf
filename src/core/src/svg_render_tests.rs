//! SVG images drawn as vector (`svg.rs`). PROTOTYPE: the files come from the folder `LPDF_SVG_DIR` names, each drawn on
//! a page into `LPDF_SVG_OUT`; both unset, the test does nothing. The fixtures of the finished feature are committed.

use std::path::PathBuf;

use crate::{LpdfEngine, RenderEncryption};

/// Render a document of one `<img>` of `bytes`, at its natural size, as an SDK renders it.
fn render(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut engine = LpdfEngine::new("");
    engine.load_image("pic", bytes);
    engine.render_xml(
        r#"<lpdf version="1"><assets><image name="pic" src="pic"/></assets>
        <document><section><layout><img name="pic" width="450pt"/></layout></section></document></lpdf>"#,
        None,
        &RenderEncryption::default(),
    )
}

#[test]
fn draws_the_svgs_of_a_folder_as_vector() {
    let (Some(dir), Some(out)) = (std::env::var_os("LPDF_SVG_DIR"), std::env::var_os("LPDF_SVG_OUT")) else {
        return;
    };
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the folder of SVGs")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no SVG in the folder");
    for path in names {
        let bytes = std::fs::read(&path).expect("the SVG");
        let pdf = render(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let target = PathBuf::from(&out).join(path.with_extension("pdf").file_name().expect("a file name"));
        std::fs::write(&target, &pdf).expect("the PDF");
        eprintln!("{} -> {} ({} bytes of SVG, {} of PDF)", path.display(), target.display(), bytes.len(), pdf.len());
    }
}
