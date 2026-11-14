// Isolated from the legacy form, JSON Apply, and Session inventory editing.
export function strictQuantity(text) {
  if (!/^[0-9]+$/.test(text)) throw new Error("Quantity must contain decimal digits only.");
  const value = Number(text);
  if (!Number.isSafeInteger(value) || value < 1 || value > 2147483647) {
    throw new Error("Quantity must be between 1 and 2147483647.");
  }
  return value;
}

export function verifyDownloadedBytes(original, output, receipt, value) {
  const [start, end] = receipt.quantity_span;
  if (!receipt.verified || output.length !== original.length || end !== start + 4 ||
      !Number.isSafeInteger(start) || start < 0 || end > output.length) {
    throw new Error("Invalid verified export shape.");
  }
  if (new DataView(output.buffer, output.byteOffset, output.byteLength).getInt32(start, false) !== value) {
    throw new Error("Export quantity encoding differs from the requested value.");
  }
  const changed = [];
  for (let i = 0; i < original.length; i++) {
    if (original[i] !== output[i]) {
      if (i < start || i >= end) throw new Error("Export changed an unrelated byte.");
      changed.push(i);
    }
  }
  if (JSON.stringify(changed) !== JSON.stringify(receipt.changed_offsets)) {
    throw new Error("Export byte differences disagree with the receipt.");
  }
}

export function slotFiles(files) {
  const result = new Map();
  let folder = null;
  for (const file of files) {
    const parts = file.webkitRelativePath.replaceAll("\\", "/").split("/");
    if (parts.length < 2 || parts.some(p => !p || p === "." || p === "..")) {
      throw new Error("Choose a complete save-slot folder using the folder picker.");
    }
    if (folder !== null && folder !== parts[0]) throw new Error("Choose exactly one save-slot folder.");
    folder = parts[0];
    const key = parts.slice(1).join("/").toLowerCase();
    if (result.has(key)) throw new Error(`Ambiguous case-colliding slot file: ${key}`);
    result.set(key, file);
  }
  for (const key of ["save.dat", "sfallfs.sav", "automap.sav", "sfalldb.sav", "sfallgv.sav"]) {
    if (!result.has(key)) throw new Error(`Complete slot must include ${key}.`);
  }
  return result;
}

export function wireRpuPanel(wasm) {
  const get = id => document.getElementById(`rpu-${id}`);
  const status = text => { get("status").textContent = text; };
  let editor = null;
  let input = null;
  let selected = null;
  let output = null;
  let receiptText = null;
  let epoch = 0;

  function clearOutput() {
    output = null;
    receiptText = null;
    get("download").disabled = true;
    get("receipt").disabled = true;
  }
  function reset() {
    epoch++;
    clearOutput();
    editor?.free();
    editor = null;
    input = null;
    selected = null;
    get("rows").replaceChildren();
    get("evidence").textContent = "";
    get("apply").disabled = true;
    get("inspect").disabled = false;
    status("Inputs changed. Inspect again before editing.");
  }
  for (const id of ["slot", "archive", "master", "plan", "consent"]) get(id).addEventListener("change", reset);
  get("quantity").addEventListener("input", () => {
    clearOutput();
    try { strictQuantity(get("quantity").value); get("apply").disabled = !selected; }
    catch { get("apply").disabled = true; }
  });

  async function bytes(file, max, label) {
    if (!file || file.size > max || file.size === 0) throw new Error(`${label}: missing, empty, or exceeds ${max} bytes.`);
    return new Uint8Array(await file.arrayBuffer());
  }

  get("inspect").addEventListener("click", async () => {
    reset();
    const current = epoch;
    get("inspect").disabled = true;
    try {
      if (!get("consent").checked) throw new Error("Accept the explicit offline resource assumptions first.");
      const files = slotFiles(Array.from(get("slot").files));
      const rpuFile = get("archive").files[0];
      const masterFile = get("master").files[0];
      const planFile = get("plan").files[0];
      status("Reading resources locally. Large archives may take a few moments…");
      const planBytes = await bytes(planFile, 65536, "Resource declaration");
      const declaration = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(planBytes));
      declaration.accept_offline_assumptions = true; // explicit checkbox above
      const save = await bytes(files.get("save.dat"), 64 * 1024 * 1024, "SAVE.DAT");
      const fs = await bytes(files.get("sfallfs.sav"), 4, "sfallfs.sav (empty virtual state only)");
      const overlays = [];
      let overlayTotal = 0;
      for (const [path, file] of files) {
        if (path.startsWith("proto/items/")) {
          if (!/^proto\/items\/[a-z0-9_.-]+\.pro$/.test(path)) throw new Error(`Unsupported saved item resource: ${path}`);
          overlayTotal += file.size;
          if (overlayTotal > 32_000_000 || overlays.length >= 4096) throw new Error("Saved overlay budget exceeded.");
          overlays.push({ path, bytes: Array.from(await bytes(file, 4_000_000, path)) });
        }
      }
      const rpu = await bytes(rpuFile, 512 * 1024 * 1024, "rpu.dat");
      const master = await bytes(masterFile, 512 * 1024 * 1024, "master.dat");
      if (current !== epoch) return;
      // Yield a frame so the progress message renders before synchronous WASM work.
      await new Promise(resolve => requestAnimationFrame(resolve));
      if (current !== epoch) return;
      editor = new wasm.RpuQuantityEditor(save, rpu, master, JSON.stringify(overlays), fs, JSON.stringify(declaration));
      input = save;
      const view = JSON.parse(editor.inspect());
      for (const map of view.required_map_files) {
        if (!files.has(map.toLowerCase())) throw new Error(`Incomplete slot: required map ${map} is missing.`);
      }
      const table = document.createElement("table");
      const head = table.createTHead().insertRow();
      for (const title of ["Select", "Path", "PID", "Object ID", "Quantity", "Prototype / subtype"]) {
        const th = document.createElement("th"); th.textContent = title; head.append(th);
      }
      const body = table.createTBody();
      for (const item of view.rows) {
        const row = body.insertRow();
        const radio = document.createElement("input");
        radio.type = "radio"; radio.name = "rpu-exact-object";
        radio.setAttribute("aria-label", `Select path ${JSON.stringify(item.selection.path)}, PID ${item.selection.pid}, object ${item.selection.object_id}`);
        radio.addEventListener("change", () => {
          selected = item.selection;
          clearOutput();
          get("quantity").value = String(selected.expected_quantity);
          get("apply").disabled = selected.expected_quantity < 1;
          status(`Selected exact path ${JSON.stringify(selected.path)}. Each edit starts from the uploaded quantity ${selected.expected_quantity}.`);
        });
        row.insertCell().append(radio);
        for (const text of [JSON.stringify(item.selection.path), item.selection.pid, item.selection.object_id,
          item.selection.expected_quantity, `${item.prototype_file ?? "unavailable"} / ${["Armor", "Container", "Drug", "Weapon", "Ammo", "Misc", "Key"][item.subtype] ?? "unknown"}`]) {
          row.insertCell().textContent = String(text);
        }
      }
      get("rows").replaceChildren(table);
      get("evidence").textContent = JSON.stringify(view, null, 2);
      status(`Verified ${view.rows.length} inventory objects. Select exactly one row. Quantities are serialized totals, including equipped items.`);
    } catch (error) {
      if (current === epoch) {
        editor?.free(); editor = null; input = null; selected = null;
        get("rows").replaceChildren(); get("apply").disabled = true;
        status(`Blocked: ${String(error)}`);
      }
    } finally { if (current === epoch) get("inspect").disabled = false; }
  });

  get("apply").addEventListener("click", () => {
    clearOutput();
    try {
      if (!editor || !selected || !input) throw new Error("Inspect and select an exact object first.");
      const value = strictQuantity(get("quantity").value);
      const edited = editor.edit(JSON.stringify(selected), get("quantity").value);
      const text = editor.receipt();
      const receipt = JSON.parse(text);
      verifyDownloadedBytes(input, edited, receipt, value);
      output = edited;
      receiptText = text;
      get("evidence").textContent = text;
      get("download").disabled = false;
      get("receipt").disabled = false;
      status("Edit verified by fresh structural parsing and whole-file byte comparison. Ready to download a separate file.");
    } catch (error) { status(`Blocked: ${String(error)}`); }
  });

  function download(data, name, type) {
    const url = URL.createObjectURL(new Blob([data], { type }));
    const anchor = document.createElement("a");
    anchor.href = url; anchor.download = name;
    document.body.append(anchor); anchor.click(); anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  get("download").addEventListener("click", () => {
    if (output) download(output, "RPU-quantity-edited-SAVE.DAT", "application/octet-stream");
  });
  get("receipt").addEventListener("click", () => {
    if (receiptText) download(receiptText, "RPU-quantity-edit-receipt.json", "application/json");
  });
  if (typeof wasm.RpuQuantityEditor !== "function") {
    status("RPU bindings are unavailable. Rebuild WASM and reload; legacy editing is not a fallback.");
    return;
  }
  get("controls").disabled = false;
  status("Choose a complete slot folder, both archives, and an explicit resource declaration. Nothing is uploaded to a server.");
}
