//! The setup of the benchmarks of the fixture `svg`, which `svg.rs` and `allocations.rs` share: its pictures, read from
//! the files beside it, and the document parsed.

use lpdf::{BenchDoc, BenchImages, bench_images};

use crate::common::{fixture_folder, parsed};

/// The pictures of `svg.xml`: the name its `<image>` gives each, and its file in `fixtures/`.
const PICTURES: [(&str, &str); 3] = [
    ("plot", "svg-plot.svg"),
    ("diagram", "svg-diagram.svg"),
    ("icon", "svg-icon.svg"),
];

/// The bytes of each picture of the fixture `name`, as an SDK reads its files. A file that isn't there stops the run.
pub fn files(name: &str) -> Vec<(&'static str, Vec<u8>)> {
    let folder = fixture_folder(name);
    PICTURES
        .iter()
        .map(|(picture, file)| {
            let path = folder.join(file);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| {
                panic!("the picture {picture} of {name}, {}, can't be read: {e}", path.display())
            });
            (*picture, bytes)
        })
        .collect()
}

/// `bench_images` of `files`: each SVG read and written as PDF content.
pub fn load(files: &[(&str, Vec<u8>)]) -> BenchImages {
    let loaded: Vec<(&str, &[u8])> = files.iter().map(|(name, bytes)| (*name, bytes.as_slice())).collect();
    bench_images(&loaded)
}

/// The document `name` parsed, with its pictures loaded, as an engine has them before it renders.
pub fn parsed_with_images(name: &str) -> (BenchDoc, BenchImages) {
    (parsed(name), load(&files(name)))
}
