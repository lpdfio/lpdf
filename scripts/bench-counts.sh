#!/bin/sh
# Counts the instructions, estimated cycles and allocations of one build of lpdf with benches/counts,
# as bench.yml does for each build it compares: the instructions under Valgrind (benches/counts.rs,
# data.rs and fonts.rs), the allocations with a counting allocator (benches/allocations.rs).
#
#   sh scripts/bench-counts.sh <checkout> <out folder> <name> [<profiles folder>]
#
# Copies this repo's benches/counts into <checkout>, a worktree of the build, so that every build of a
# run is measured by the same benchmarks and fixtures, and gives it <checkout>'s Cargo.lock, so that each
# build keeps the dependency versions it was released with. Turns on the split-stages feature when
# <checkout>'s lib.rs has bench_layout_doc and bench_write_pdf, the embedded-fonts feature, with the
# fonts benchmarks (benches/fonts.rs), when it has bench_layout_doc_with_fonts, and the markdown and
# code features when its schema has a <markdown> and a <code>. Writes to <out folder>:
#
#   summary.jsonl       gungraun's JSON line for each benchmark: instructions and estimated cycles
#   allocations.jsonl   a JSON line for each benchmark: allocations, bytes allocated and peak memory
#   build.json          {"name": <name>, "commit": <checkout's HEAD>}
#
# which scripts/bench-counts-report.mjs reads. Given a <profiles folder>, it keeps there the profiles
# of each benchmark, to read function by function: Gungraun's output folder, with the Callgrind
# profiles of the counted run (callgrind_annotate) and, from a second run under DHAT with whole stacks,
# the DHAT profiles (dh_view.html), with dhat.jsonl, gungraun's JSON line for each.
#
# Needs Linux on x86-64, Valgrind, Node, and the gungraun-runner of the version
# benches/counts/Cargo.toml pins:
#   cargo install gungraun-runner --version <that version> --locked
set -eu

if [ $# -ne 3 ] && [ $# -ne 4 ]; then
  echo "usage: sh scripts/bench-counts.sh <checkout> <out folder> <name> [<profiles folder>]" >&2
  exit 2
fi
CHECKOUT=$(cd "$1" && pwd)
mkdir -p "$2"
OUT=$(cd "$2" && pwd)
NAME=$3
PROFILES=""
if [ $# -eq 4 ]; then
  mkdir -p "$4"
  PROFILES=$(cd "$4" && pwd)
fi
HARNESS=$(cd "$(dirname "$0")/../benches/counts" && pwd)
LIB="$CHECKOUT/src/core/src/lib.rs"
SCHEMA="$CHECKOUT/schema/lpdf.xsd"

if [ "$HARNESS" != "$CHECKOUT/benches/counts" ]; then
  rm -rf "$CHECKOUT/benches/counts"
  mkdir -p "$CHECKOUT/benches/counts"
  cp "$HARNESS/Cargo.toml" "$CHECKOUT/benches/counts/"
  cp -R "$HARNESS/benches" "$HARNESS/fixtures" "$CHECKOUT/benches/counts/"
fi
cp "$CHECKOUT/Cargo.lock" "$CHECKOUT/benches/counts/Cargo.lock"

FEATURES=""
BENCHES="--bench counts --bench data"
WITH=""
if grep -q 'pub fn bench_layout_doc_with_fonts' "$LIB"; then
  FEATURES="embedded-fonts"
  BENCHES="$BENCHES --bench fonts"
  WITH=", with layout and the PDF write apart and the embedded fonts"
elif grep -q 'pub fn bench_layout_doc' "$LIB" && grep -q 'pub fn bench_write_pdf' "$LIB"; then
  FEATURES="split-stages"
  WITH=", with layout and the PDF write apart"
fi
for element in markdown code; do
  if grep -q "<xs:element name=\"$element\">" "$SCHEMA"; then
    FEATURES="${FEATURES:+$FEATURES,}$element"
    WITH="$WITH, with <$element>"
  fi
done
FEATURES="${FEATURES:+--features $FEATURES}"

COMMIT=$(git -C "$CHECKOUT" rev-parse HEAD)
echo ">>> $NAME ($COMMIT)$WITH"

# Each build in a target of its own, in its checkout: Cargo gives two checkouts of the same crate the
# same file in a shared one, and the second build's benchmarks would run for both.
cd "$CHECKOUT/benches/counts"
# shellcheck disable=SC2086 # FEATURES is empty or two words, BENCHES four or six
cargo bench $BENCHES $FEATURES -- --output-format=json --parallel=auto > "$OUT/summary.jsonl"
# shellcheck disable=SC2086
cargo bench --bench allocations $FEATURES > "$OUT/allocations.jsonl"

if [ -n "$PROFILES" ]; then
  # DHAT only here, after the counted run: the instruction runs keep Callgrind only. 500 frames,
  # Valgrind's most, since DHAT drops an allocation whose stack it cuts off below the benchmark's frame.
  # The profiles are for reading, so a DHAT run that fails leaves the counts measured.
  # shellcheck disable=SC2086
  if ! cargo bench $BENCHES $FEATURES -- --default-tool=dhat --dhat-args=--num-callers=500 \
    --output-format=json --parallel=auto > "$PROFILES/dhat.jsonl"; then
    echo "::warning::$NAME: the DHAT run failed, so its DHAT profiles are not all there"
  fi
  cp -R target/gungraun/. "$PROFILES/" || echo "::warning::$NAME: its profiles could not be kept"
  echo ">>> $NAME: profiles in $PROFILES ($(du -sh "$PROFILES" | cut -f1))"
fi

node -e 'process.stdout.write(JSON.stringify({ name: process.argv[1], commit: process.argv[2] }) + "\n")' \
  "$NAME" "$COMMIT" > "$OUT/build.json"
echo ">>> $NAME: $(wc -l < "$OUT/summary.jsonl") benchmarks in $OUT"
