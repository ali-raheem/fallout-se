"""Build the static WASM viewer/editor without Trunk (Python 3 + Rust required)."""
from pathlib import Path
import os
import re
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
web = root / "crates" / "fallout_web"
dist = web / "dist"
lock = (root / "Cargo.lock").read_text(encoding="utf-8")
version = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"', lock).group(1)
local = root / "target" / "wasm-tools" / "bin" / ("wasm-bindgen.exe" if os.name == "nt" else "wasm-bindgen")
binding = str(local) if local.is_file() else shutil.which("wasm-bindgen")
if not binding:
    raise SystemExit(f"Install bindings: cargo install wasm-bindgen-cli --version {version} --locked --root target/wasm-tools")
actual = subprocess.check_output([binding, "--version"], text=True).strip().split()[-1]
if actual != version:
    raise SystemExit(f"wasm-bindgen {version} required by Cargo.lock; found {actual}")
subprocess.run(["cargo", "build", "--locked", "-p", "fallout-se-web", "--target", "wasm32-unknown-unknown", "--release"], cwd=root, check=True)
dist.mkdir(exist_ok=True)
subprocess.run([binding, "--target", "web", "--out-dir", str(dist), "--out-name", "fallout-se-web",
                str(root / "target/wasm32-unknown-unknown/release/fallout_web.wasm")], cwd=root, check=True)
for name in ["editor.html", "editor.js", "rpu-editor.js", "rpu-resource-declaration.example.json", "index.js"]:
    shutil.copyfile(web / name, dist / name)
index = (web / "index.html").read_text(encoding="utf-8")
index = index.replace('<base data-trunk-public-url />', '<base href="./" />')
index = re.sub(r'^.*<link data-trunk[^>]+/>\s*\n', '', index, flags=re.MULTILINE)
index = index.replace('<script data-trunk ', '<script ')
(dist / "index.html").write_text(index, encoding="utf-8")
print("Built", dist)
print("Serve: python -m http.server 8765 --bind 127.0.0.1 --directory crates/fallout_web/dist")
print("Open http://127.0.0.1:8765/editor.html")
