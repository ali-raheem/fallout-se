const MAX_UPLOAD_BYTES = 16 * 1024 * 1024;
const WASM_BUNDLE_BASENAME = "fallout-se-web";

let wasmBindings = null;

const elements = {
  dropZone: document.getElementById("drop-zone"),
  fileInput: document.getElementById("file-input"),
  chooseFile: document.getElementById("choose-file"),
  copyJson: document.getElementById("copy-json"),
  formatJson: document.getElementById("format-json"),
  applyJson: document.getElementById("apply-json"),
  downloadSave: document.getElementById("download-save"),
  jsonEditor: document.getElementById("json-editor"),
  status: document.getElementById("status"),
};

const state = {
  originalBytes: null,
  updatedBytes: null,
  filenameBase: "save",
  outputFilename: "SAVE_edited.DAT",
  ready: false,
  initError: null,
};

function setStatus(message, kind = "info") {
  elements.status.textContent = message;
  elements.status.dataset.kind = kind;
}

function normalizeFilename(value) {
  const cleaned = value.replace(/\.[^/.]+$/, "").replace(/[^a-zA-Z0-9_-]+/g, "_");
  return cleaned.length > 0 ? cleaned : "save";
}

function resetEditor() {
  state.originalBytes = null;
  state.updatedBytes = null;
  state.filenameBase = "save";
  state.outputFilename = "SAVE_edited.DAT";
  elements.jsonEditor.value = "";
  elements.copyJson.disabled = true;
  elements.formatJson.disabled = true;
  elements.applyJson.disabled = true;
  elements.downloadSave.disabled = true;
}

function extractErrorMessage(error) {
  if (error && typeof error === "object") {
    if (typeof error.message === "string") {
      return error.message;
    }
    if (typeof error.code === "string") {
      return `Web editor error: ${error.code}`;
    }
  }
  return String(error ?? "Unknown error");
}

function baseHref() {
  const base = document.querySelector("base");
  if (base && typeof base.href === "string" && base.href.length > 0) {
    return base.href;
  }
  return document.baseURI;
}

function coerceUint8Array(value) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) {
    return new Uint8Array(value);
  }
  return null;
}

async function loadSaveFile(file) {
  if (state.initError) {
    setStatus(`WASM failed to load: ${state.initError}`, "error");
    return;
  }

  if (!state.ready) {
    setStatus("WASM engine is still loading...", "info");
    return;
  }

  if (!file) {
    setStatus("No file selected.", "error");
    return;
  }

  if (file.size > MAX_UPLOAD_BYTES) {
    resetEditor();
    setStatus("File is too large. Maximum supported size is 16 MiB.", "error");
    return;
  }

  const lower = file.name.toLowerCase();
  if (!lower.endsWith("save.dat") && !lower.endsWith(".dat")) {
    setStatus("Selected file does not look like SAVE.DAT; attempting parse anyway.", "info");
  } else {
    setStatus(`Reading ${file.name}...`, "info");
  }

  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const options = {
      metadata: null,
    };
    const renderedJson = wasmBindings.export_save_json(bytes, options);

    state.originalBytes = bytes;
    state.updatedBytes = null;
    state.filenameBase = normalizeFilename(file.name);
    state.outputFilename = `${state.filenameBase}_edited.SAVE.DAT`;

    elements.jsonEditor.value = renderedJson;
    elements.copyJson.disabled = false;
    elements.formatJson.disabled = false;
    elements.applyJson.disabled = false;
    elements.downloadSave.disabled = true;
    setStatus(`Loaded ${file.name}. Edit JSON and apply when ready.`, "ok");
  } catch (error) {
    resetEditor();
    setStatus(extractErrorMessage(error), "error");
  }
}

async function copyJson() {
  const text = elements.jsonEditor.value;
  if (!text) {
    return;
  }

  try {
    await navigator.clipboard.writeText(text);
    setStatus("JSON copied to clipboard.", "ok");
  } catch (_error) {
    setStatus("Clipboard access denied by the browser.", "error");
  }
}

function formatJson() {
  const text = elements.jsonEditor.value.trim();
  if (!text) {
    setStatus("No JSON to format.", "error");
    return;
  }

  try {
    const parsed = JSON.parse(text);
    elements.jsonEditor.value = JSON.stringify(parsed, null, 2);
    setStatus("JSON formatted.", "ok");
  } catch (error) {
    setStatus(`Invalid JSON: ${extractErrorMessage(error)}`, "error");
  }
}

function parseApplyPayload(payload) {
  if (!payload || typeof payload !== "object") {
    throw new Error("Invalid apply response from wasm.");
  }

  const bytes = coerceUint8Array(payload.updated_bytes);
  if (!bytes || bytes.length === 0) {
    throw new Error("Apply response did not include updated save bytes.");
  }

  const filenameHint =
    typeof payload.filename_hint === "string" && payload.filename_hint.length > 0
      ? payload.filename_hint
      : `${state.filenameBase}_edited.SAVE.DAT`;

  return {
    bytes,
    filenameHint,
  };
}

function applyJsonAndBuildSave() {
  if (!state.originalBytes || state.originalBytes.length === 0) {
    setStatus("Load a SAVE.DAT before applying JSON edits.", "error");
    return;
  }

  const editedJson = elements.jsonEditor.value.trim();
  if (!editedJson) {
    setStatus("Editor JSON is empty.", "error");
    return;
  }

  try {
    const options = {
      metadata: null,
    };
    const payload = wasmBindings.apply_json_to_save(state.originalBytes, editedJson, options);
    const { bytes, filenameHint } = parseApplyPayload(payload);

    state.updatedBytes = bytes;
    state.outputFilename = filenameHint;
    elements.downloadSave.disabled = false;
    setStatus(`Applied JSON edits. Ready to download ${state.outputFilename}.`, "ok");
  } catch (error) {
    state.updatedBytes = null;
    elements.downloadSave.disabled = true;
    setStatus(extractErrorMessage(error), "error");
  }
}

function downloadEditedSave() {
  if (!state.updatedBytes || state.updatedBytes.length === 0) {
    return;
  }

  const blob = new Blob([state.updatedBytes], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = state.outputFilename;
  anchor.click();
  URL.revokeObjectURL(url);
  setStatus(`Saved ${state.outputFilename}.`, "ok");
}

function wireDragAndDrop() {
  const activate = () => elements.dropZone.classList.add("active");
  const deactivate = () => elements.dropZone.classList.remove("active");

  ["dragenter", "dragover"].forEach((eventName) => {
    elements.dropZone.addEventListener(eventName, (event) => {
      event.preventDefault();
      activate();
    });
  });

  ["dragleave", "dragend"].forEach((eventName) => {
    elements.dropZone.addEventListener(eventName, (event) => {
      event.preventDefault();
      deactivate();
    });
  });

  elements.dropZone.addEventListener("drop", (event) => {
    event.preventDefault();
    deactivate();
    const file = event.dataTransfer?.files?.[0];
    void loadSaveFile(file);
  });

  elements.dropZone.addEventListener("click", () => {
    elements.fileInput.click();
  });

  elements.dropZone.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      elements.fileInput.click();
    }
  });
}

function wireControls() {
  elements.chooseFile.addEventListener("click", () => elements.fileInput.click());
  elements.fileInput.addEventListener("change", () => {
    const file = elements.fileInput.files?.[0];
    void loadSaveFile(file);
  });
  elements.copyJson.addEventListener("click", () => {
    void copyJson();
  });
  elements.formatJson.addEventListener("click", formatJson);
  elements.applyJson.addEventListener("click", applyJsonAndBuildSave);
  elements.downloadSave.addEventListener("click", downloadEditedSave);
}

async function initWasm() {
  const base = baseHref();
  const jsUrl = new URL(`${WASM_BUNDLE_BASENAME}.js`, base).toString();
  const wasmUrl = new URL(`${WASM_BUNDLE_BASENAME}_bg.wasm`, base).toString();

  const mod = await import(jsUrl);
  if (typeof mod.default !== "function") {
    throw new Error(`WASM bundle missing default init(): ${jsUrl}`);
  }

  await mod.default({ module_or_path: wasmUrl });
  wasmBindings = mod;
  state.ready = true;
}

async function main() {
  resetEditor();
  wireDragAndDrop();
  wireControls();
  setStatus("Loading WASM editor...", "info");

  try {
    await initWasm();
    setStatus("Drop a SAVE.DAT file to begin editing.", "info");
  } catch (error) {
    const message = extractErrorMessage(error);
    state.initError = message;
    setStatus(`Failed to initialize WASM editor: ${message}`, "error");
  }
}

void main();
