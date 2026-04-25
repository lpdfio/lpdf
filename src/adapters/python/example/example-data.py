"""
example-data.py — render data-invoice.xml with dynamic data from data-invoice.json.

Run (after 'make build-adapter-python'):
  docker run --rm \\
    -v "$(pwd)/src/adapters/python/lpdf:/app/lpdf" \\
    -v "$(pwd)/src/adapters/python/example:/app/example" \\
    -v "$(pwd)/example:/app/example-data" \\
    -v "$(pwd)/src/adapters/python/resources:/app/resources" \\
    -w /app lpdf-python python example/example-data.py

Output: example/result/example-data-python.pdf
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lpdf import LpdfEngine
from lpdf.engine import RenderOptions

_docker_root = Path("/app/example-data")
root = _docker_root if _docker_root.exists() else Path(__file__).resolve().parents[4] / "example"

xml_file  = root / "xml" / "data-invoice.xml"
json_file = root / "xml" / "data-invoice.json"
output_file = "example-data-python.pdf"

xml  = xml_file.read_text(encoding="utf-8")
data = json.loads(json_file.read_bytes())

engine = LpdfEngine("")  # empty key → free tier (watermark)

pdf = engine.render_pdf(xml, options=RenderOptions(data=data))

(root / "result" / output_file).write_bytes(pdf)
print(f"output: {output_file} ({len(pdf):,} bytes)")
