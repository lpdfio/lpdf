"""
Shared snapshot-test helpers for the Python adapter.
Centralises path constants, SHA-256 hashing, and compare-or-update logic so
that test_snapshot.py only contains test setup and engine invocations.
"""

import hashlib
import os
from pathlib import Path

ROOT      = Path("/app")
FIXTURES  = ROOT / "test" / "fixtures"
SNAPSHOTS = ROOT / "test" / "snapshots"

EXAMPLES = [f"example{i}" for i in range(1, 12)]


def compare_or_update(name: str, pdf_bytes: bytes) -> None:
    """Compare the SHA-256 hash of *pdf_bytes* against the stored snapshot for
    *name*, or write a new snapshot when ``UPDATE_SNAPSHOTS=1``."""
    sha  = hashlib.sha256(pdf_bytes).hexdigest()
    snap = SNAPSHOTS / f"{name}.pdf.sha256"

    if os.environ.get("UPDATE_SNAPSHOTS") == "1":
        snap.write_text(sha)
    else:
        stored = snap.read_text().strip()
        assert sha == stored, (
            f"Snapshot mismatch for {name}. "
            f"Run with UPDATE_SNAPSHOTS=1 to accept.\n"
            f"  expected: {stored}\n  received: {sha}"
        )
