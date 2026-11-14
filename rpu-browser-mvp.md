# Experimental RPU browser quantity editor (Step 4B.2N)

The existing browser/WASM editor now has a separate experimental RPU quantity panel. It runs locally in a desktop browser on a static HTTP site: no native GUI, local editing service, or Windows filesystem API is used. Do not use the legacy JSON/inventory editor for RPU quantity editing. Opening experimental mode clears/disables legacy controls; the two editing states never share an output.

## Build and launch (Windows 11)

Prerequisites: Rust/Cargo/rustup, Python 3, and a current 64-bit desktop browser with directory upload support. Run these commands in PowerShell from the project root:

```powershell
cd C:\FalloutRE\src\fallout-se
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.108 --locked --root target/wasm-tools
python tools/build_web.py
python -m http.server 8765 --bind 127.0.0.1 --directory crates/fallout_web/dist
```

Open `http://127.0.0.1:8765/editor.html` manually. If port 8765 is already serving this project, reuse it; otherwise choose another port and update the URL. The binding-generator version above matches the current Cargo.lock; the build script checks that match. It builds the actual release WASM and copies static assets, including the existing viewer and editor. Build outputs are in the ignored dist/target directories. The Python HTTP server only serves static assets; selected save/resource bytes are not sent to it.

## Build and launch (macOS Apple Silicon or Intel; Linux)

Use a native Rust installation for your machine, Python 3 and a current 64-bit desktop browser with directory uploads. From your checkout directory:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.108 --locked --root target/wasm-tools
python3 tools/build_web.py
python3 -m http.server 8765 --bind 127.0.0.1 --directory crates/fallout_web/dist
```

Open `http://127.0.0.1:8765/editor.html`. The same wasm32 artifact is used on all hosts; the build helper selects the host binding-generator executable. Paths are selected in the browser, not hardcoded for Windows. macOS and Linux runtime testing has **not** been performed in this step. The WASM target was built and exercised in Chrome on Windows. A browser directory picker is required; browsers without it are outside this MVP. Large archives need substantial available memory (multiple copies of roughly 462 MB of baseline input can coexist during inspection), so this is a desktop workflow, not a mobile-memory claim.

The existing Trunk deployment workflow also remains supported: index.html now copies rpu-editor.js and the resource-declaration example along with the prior editor assets. Trunk itself was not run locally in this step; the direct build helper above was run successfully.

## Use the editor

1. Expand **Experimental Fallout 2 RPU quantity editor**. The legacy form below becomes inactive.
2. Choose the **complete save-slot folder**, preserving its relative hierarchy. For the retained test candidate, select `rpu-runtime-test/SLOT-CANDIDATE`. Do not select only SAVE.DAT. Other platforms can use an unchanged copy of that complete directory. The picker is read-only.
3. Choose your intended `rpu.dat` and `master.dat` archives. The explicit offline order is RPU first, base second. No filesystem discovery, configured extra mods, loose resource fallback or runtime hooks are imported.
4. Supply a **resource declaration JSON**. Download the example from the panel or use `crates/fallout_web/rpu-resource-declaration.example.json`. Its expected hashes identify the archives used in the verified runtime baseline:
   - rpu.dat: `27db830e0fb67d02b60e166e2acb772a92292f5f4dd88f733b740b406a0864ee`
   - master.dat: `9b096d3035edafd4077deeb8ee7877a803b9db98497aa4596624b5c058a84711`

   For another environment, obtain trusted expected hashes and describe your selected offline resources; do not replace a mismatched hash merely to bypass rejection. Hash matching authenticates bytes against the declaration, not runtime mount order or the historical installation. Unknown declaration fields are rejected. The explicit checkbox supplies `accept_offline_assumptions=true` for this inspection; the example defaults to false.
5. Check the complete-slot/offline-environment assertion and click **Inspect RPU inventory**. Failure blocks editing; there is no legacy fallback. The UI requires SAVE.DAT, AUTOMAP.SAV, sfalldb.sav, sfallgv.sav and exactly empty four-byte sfallfs.sav, verifies presence of the save prefix's referenced map files, and captures every saved item .pro under proto/items. Case-colliding paths, unsupported virtual state and unresolved required metadata are rejected. Maps, critter prototypes and nonvirtual sfall state are retained as companion context, not fully simulated by the editor.
6. Select an exact row. Rows show full nested inventory path, PID, object ID, serialized quantity, resolved prototype filename and subtype. Paths distinguish duplicate PIDs and even duplicate object IDs. Localized names are not yet resolved. For the baseline candidate expect PID 591 / ID 15 / [0] / quantity 2 and PID 7 / ID 11 / [1] / quantity 10001. The game's 10000 inventory + one equipped display corresponds to serialized total 10001; the editor does not split equipment display counts.
7. Enter an integer from **1 through 2147483647** and click **Verify quantity edit**. No zero-to-delete, add/remove, merge, equipment edits, PID edits, or multi-row transaction is offered. Every edit/export starts from the same immutable uploaded snapshot; previous exports are not silently accumulated. Reopen a complete slot snapshot to work from a different baseline.
8. Download **RPU-quantity-edited-SAVE.DAT** and optionally its JSON verification receipt. Input files are never written. Changing the selection, quantity, resources, consent or save invalidates any previously offered output. Each result is checked by fresh structural parsing, exact expected-value/identity checks and complete byte comparison outside the selected four-byte quantity field. The JS UI also independently verifies the output encoding and byte differences before enabling download.
9. Save the download to a separate location. **Browser downloads do not provide the native Windows exporter's filesystem no-overwrite, alias or protected-directory guarantees.** The application does not request writable file handles, but a user-controlled download dialog can choose an existing file; do not select an original or live game slot. It never installs the download automatically. A later manual test requires all matching companions, not just SAVE.DAT.

## Implementation boundary

`core_api/browser_rpu.rs` is a narrow feature-gated upload/inspection/edit facade. It holds immutable ByteDocument/resource snapshots, validates declared archive hashes, builds the existing RpuSourcePlan, and calls diagnose/traverse. Edit calls the existing authorize_experiment -> edit_copy -> verify_export_bytes chain. That chain uses the transactional four-byte patcher, re-traverses before/after, compares the complete hierarchy, and verifies a fresh output document. Verifier and cursor share their wire walker; their fresh parses are independent invocations, not separate grammar implementations. The browser's independent byte-comparison check detects unrelated modifications without trusting that walker.

No Session inventory getters/setters, apply_character, GameObject serializer, filesystem read/write, or native exporter is used in this facade. The WASM wrapper owns its facade instance and parses quantity text before wasm integer conversion. Selection is bound to an instance-unique snapshot token, environment digest, full path, PID, object ID and expected original count. Resources cannot mutate after construction. The feature uses existing serde/serde_json dependencies; no new runtime framework was added. Platform-specific export remains isolated; `cargo tree --target wasm32-unknown-unknown` contains no windows-sys dependency.

## Tested result

- Windows Chrome headless **157.0.8103.52**: actual folder upload, real baseline rpu/master archives, SHA assertions, exact row display, 10001 -> 10002 edit, actual binary download and receipt download passed.
- Download length 66,290; only offset 36643 differs from input, inside span 36640..36644. The other item, header, payloads and every unrelated byte are unchanged. Output SHA-256: `68890965b8de4c749d8548d2c1233f5da8958c75dbf90fab4e0c0e7474f11005`.
- UI rejects empty/zero/negative/fraction/exponent/whitespace/overflow quantities, invalidates old output on selection/input changes, and blocks a mismatched resource hash with no fallback. Closing the experimental panel restores legacy Fallout 1 and Fallout 2 upload/display in the actual browser.
- Four portable core facade tests: verified candidate display/edit/no-op/reparse; duplicate PID+ID and nested path selection; stale snapshots/environment/identity/count and invalid integer rejection; missing/unsupported/unasserted metadata, wrong hashes, malformed saves, nonempty virtual state and duplicate overlays.
- Three Node unit tests: strict text validation, complete-folder/collision validation, independent output byte/receipt checks.
- `cargo test --workspace`: **350 passed, 0 failed, 13 ignored**.
- Release WASM build, `cargo fmt --all -- --check`, and `git diff --check`: passed. Existing unused GUI/TUI dependency and Git line-ending conversion notices are unrelated.
- All real game files, original fixture files and staged slot files were hash-checked unchanged after testing. Browser test downloads went only to a new test-owned temporary directory. No game launch/control, live-slot write, commit or native GUI implementation occurred.

Reproduce checks:

```sh
cargo test -p fallout-se-core --features experimental-rpu-browser --lib browser_rpu
cargo test -p fallout-se-web --lib
node --test crates/fallout_web/rpu-editor.test.mjs
cargo test --workspace
cargo fmt --all -- --check
git diff --check
python3 tools/build_web.py
```

Use `python` on Windows. Optional real-resource browser test (requires Node and Playwright; installation stays in ignored target):

```sh
npm install --prefix target/browser-tests playwright
# Either install Playwright Chromium or provide an existing Chrome executable as the last argument.
npx --prefix target/browser-tests playwright install chromium
node tools/rpu-browser-e2e.cjs http://127.0.0.1:8765/editor.html /path/to/SLOT-CANDIDATE /path/to/rpu.dat /path/to/master.dat
```

On Windows use `npm.cmd` / `npx.cmd` if PowerShell blocks their .ps1 shims. The actual Windows test used installed Chrome as its last argument and did not install another browser. This opt-in test requires the baseline candidate hash, writes test results to rpu-browser-e2e-result.json, screenshot to ignored target/rpu-browser-mvp.png, and keeps downloaded evidence in a new temporary directory. It never writes its input folder or archives. Serving the build at the URL is a separate prerequisite.

## Remaining release work

The MVP is usable now for the declared supported offline environment. Before calling it a broadly supported release: run the documented flow on macOS Apple Silicon/Intel and Linux, check additional target browsers and memory budgets, document/version accepted resource plans, and retain regression evidence for other supported saves. Localized names, batch editing, add/remove/equip, runtime source authentication, nonempty virtual filesystem support and automatic slot installation remain outside this quantity-only MVP. Do not assume the browser's download semantics equal the native exporter's guarantees. The user's existing native game round-trip validates the earlier byte edit; the newly downloaded 10002 output was structurally/byte verified, not launched in the game during this step.
