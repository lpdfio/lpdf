"""
encrypt-open-password.py — render showcase-encryption.xml with RC4-128 encryption,
open password required, copy disabled.

Run (after 'make build-adapter-python'):
    docker run --rm \\
        -v "$(pwd)/src/adapters/python/lpdf:/app/lpdf" \\
        -v "$(pwd)/src/adapters/python/example:/app/example" \\
        -v "$(pwd)/example:/app/example-data" \\
        -v "$(pwd)/docs:/app/docs" \\
        -v "$(pwd)/src/adapters/python/resources:/app/resources" \\
        -w /app lpdf-python python example/encrypt-open-password.py

Output: example/result/encrypt-open-password-python.pdf
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lpdf import LpdfEngine

_docker_data = Path("/app/example-data")
root = _docker_data if _docker_data.exists() else Path(__file__).resolve().parents[4] / "example"

_docker_docs = Path("/app/docs")
xml_file = _docker_docs / "examples/showcase-encryption.xml" \
    if _docker_docs.exists() \
    else Path(__file__).resolve().parents[4] / "docs/examples/showcase-encryption.xml"

output_file = "encrypt-open-password-python.pdf"

xml = xml_file.read_text(encoding="utf-8")

engine = LpdfEngine("")  # empty key → free tier (watermark)

# With open password — viewers prompt for "password" before displaying content.
engine.set_encryption("password", "owner", {"copy": False})

pdf = engine.render_pdf(xml)

(root / "result" / output_file).write_bytes(pdf)
print(f"output: {output_file} ({len(pdf):,} bytes)")
