import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from lpdf import LpdfEngine

# init engine
engine = LpdfEngine("")  # empty key → free tier (watermark)

# optional: load fonts and assets
# engine.load_font("Inter", Path("/path/to/Inter.ttf").read_bytes())

input_file = "invoice.xml"
output_file = "invoice-python.pdf"

root = Path("/app/example-data")

# load xml from file
xml = (root / input_file).read_text(encoding="utf-8")

# render pdf from xml
pdf = engine.render_pdf(xml)

# write pdf to file
(root / output_file).write_bytes(pdf)

print(f"output: {output_file} ({len(pdf):,} bytes)")
