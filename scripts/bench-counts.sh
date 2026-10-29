#!/bin/sh
# Counts the instructions, estimated cycles and allocations of one build of lpdf with benches/counts,
# as bench.yml does for each build it compares: the instructions under Valgrind (benches/counts.rs),
# the allocations with a counting allocator (benches/allocations.rs).
#
#   sh scripts/bench-counts.sh <checkout> <out folder> <name>
#
# Copies this repo's benches/counts into <checkout>, a worktree of the build, so that every build of a
# run is measured by the same benchmarks, and gives it <checkout>'s Cargo.lock, so that each build
# keeps the dependency versions it was released with. Turns on the split-stages feature when
# <checkout>'s lib.rs has bench_layout_doc and bench_write_pdf. Writes to <out folder>:
#
#   summary.jsonl       gungraun's JSON line for each benchmark: instructions and estimated cycles
#   allocations.jsonl   a JSON line for each benchmark: allocations, bytes allocated and peak memory
#   build.json          {"name": <name>, "commit": <checkout's HEAD>}
#
# which scripts/bench-counts-report.mjs reads. Needs Linux on x86-64, Valgrind, Node, and the
# gungraun-runner of the version benches/counts/Cargo.toml pins:
#   cargo install gungraun-runner --version <that version> --locked
set -eu

if [ $# -ne 3 ]; then
  echo "usage: sh scripts/bench-counts.sh <checkout> <out folder> <name>" >&2
  exit 2
fi
CHECKOUT=$(cd "$1" && pwd)
mkdir -p "$2"
OUT=$(cd "$2" && pwd)
NAME=$3
HARNESS=$(cd "$(dirname "$0")/../benches/counts" && pwd)
LIB="$CHECKOUT/src/core/src/lib.rs"

if [ "$HARNESS" != "$CHECKOUT/benches/counts" ]; then
  rm -rf "$CHECKOUT/benches/counts"
  mkdir -p "$CHECKOUT/benches/counts"
  cp "$HARNESS/Cargo.toml" "$CHECKOUT/benches/counts/"
  cp -R "$HARNESS/benches" "$CHECKOUT/benches/counts/"
fi
cp "$CHECKOUT/Cargo.lock" "$CHECKOUT/benches/counts/Cargo.lock"

FEATURES=""
if grep -q 'pub fn bench_layout_doc' "$LIB" && grep -q 'pub fn bench_write_pdf' "$LIB"; then
  FEATURES="--features split-stages"
fi

COMMIT=$(git -C "$CHECKOUT" rev-parse HEAD)
echo ">>> $NAME ($COMMIT)${FEATURES:+, with layout and the PDF write apart}"

# Each build in a target of its own, in its checkout: Cargo gives two checkouts of the same crate the
# same file in a shared one, and the second build's benchmarks would run for both.
cd "$CHECKOUT/benches/counts"
# shellcheck disable=SC2086 # FEATURES is empty or two words
cargo bench --bench counts $FEATURES -- --output-format=json --parallel=auto > "$OUT/summary.jsonl"
# shellcheck disable=SC2086
cargo bench --bench allocations $FEATURES > "$OUT/allocations.jsonl"

node -e 'process.stdout.write(JSON.stringify({ name: process.argv[1], commit: process.argv[2] }) + "\n")' \
  "$NAME" "$COMMIT" > "$OUT/build.json"
echo ">>> $NAME: $(wc -l < "$OUT/summary.jsonl") benchmarks in $OUT"
