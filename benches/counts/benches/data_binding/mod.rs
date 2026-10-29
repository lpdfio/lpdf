//! The setup of the benchmarks of `data`, which `data.rs` and `allocations.rs` share: the fixture with its
//! data, parsed, bound or laid out.

use std::path::PathBuf;

use lpdf::{BenchDoc, bench_data_apply};
#[cfg(feature = "split-stages")]
use lpdf::{BenchPages, bench_layout_doc};

use crate::common::{fixture, parsed};

/// The data of the fixture `data`, `fixtures/data.json`.
fn fixture_data() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/data.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the data {} can't be read: {e}", path.display()))
}

pub fn fixture_with_data(name: &str) -> (String, String) {
    (fixture(name), fixture_data())
}

pub fn parsed_with_data(name: &str) -> (BenchDoc, String) {
    (parsed(name), fixture_data())
}

/// The fixture parsed and its data bound: what layout gets.
pub fn bound(name: &str) -> BenchDoc {
    bench_data_apply(parsed(name), &fixture_data())
        .unwrap_or_else(|e| panic!("{name} doesn't take its data: {e}"))
}

#[cfg(feature = "split-stages")]
pub fn bound_laid_out(name: &str) -> BenchPages {
    bench_layout_doc(bound(name))
}
