"""Assert the packaged desktop entry stays visible with exact MIME support."""

from pathlib import Path
import sys

desktop = Path(sys.argv[1]).read_text()
assert "NoDisplay=true" not in desktop
assert "Hidden=true" not in desktop
mime = None
for line in desktop.splitlines():
    if line.startswith("MimeType="):
        mime = [part for part in line.split("=", 1)[1].strip().strip(";").split(";") if part]
assert mime == [
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "text/html",
    "application/xhtml+xml",
], mime
assert "Categories=" in desktop and "Settings" in desktop
print("desktop entry remains visible with exact HTTP HTTPS HTML XHTML support")
