const MAX_UPLOAD_BYTES = 16 * 1024 * 1024;
const WASM_BUNDLE_BASENAME = "fallout-se-web";

let wasmBindings = null;
let wasmInitInProgress = false;
let wasmInitTimer = null;

const elements = {
  dropZone: document.getElementById("drop-zone"),
  fileInput: document.getElementById("file-input"),
  chooseFile: document.getElementById("choose-file"),
  copyOutput: document.getElementById("copy-output"),
  downloadOutput: document.getElementById("download-output"),
  verboseToggle: document.getElementById("verbose-toggle"),
  output: document.getElementById("output"),
  status: document.getElementById("status"),
};

const state = {
  renderedText: "",
  outputFilename: "sheet.txt",
  ready: false,
  initError: null,
};

function setStatus(message, kind = "info") {
  elements.status.textContent = message;
  elements.status.dataset.kind = kind;
}

function resetOutput() {
  state.renderedText = "";
  state.outputFilename = "sheet.txt";
  elements.output.textContent = "";
  elements.copyOutput.disabled = true;
  elements.downloadOutput.disabled = true;
}

function setRenderedOutput(text, filenameBase) {
  state.renderedText = text;
  state.outputFilename = `${filenameBase}_sheet.txt`;
  elements.output.textContent = text;
  elements.copyOutput.disabled = false;
  elements.downloadOutput.disabled = false;
}

function normalizeFilename(value) {
  const cleaned = value.replace(/\.[^/.]+$/, "").replace(/[^a-zA-Z0-9_-]+/g, "_");
  return cleaned.length > 0 ? cleaned : "save";
}

function extractErrorMessage(error) {
  if (error && typeof error === "object") {
    if (typeof error.message === "string") {
      return error.message;
    }
    if (typeof error.code === "string") {
      return `Web render error: ${error.code}`;
    }
  }
  return String(error ?? "Unknown error");
}

async function renderFile(file) {
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
    resetOutput();
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
      verbose: elements.verboseToggle.checked,
      metadata: null,
    };

    const renderedText = wasmBindings.render_save_text(bytes, options);
    setRenderedOutput(renderedText, normalizeFilename(file.name));
    setStatus(`Rendered ${file.name}.`, "ok");
  } catch (error) {
    resetOutput();
    setStatus(extractErrorMessage(error), "error");
  }
}

function baseHref() {
  const base = document.querySelector("base");
  if (base && typeof base.href === "string" && base.href.length > 0) {
    return base.href;
  }
  return document.baseURI;
}

async function initWasmFallback() {
  if (wasmInitInProgress || state.ready || state.initError) {
    return;
  }

  wasmInitInProgress = true;
  const base = baseHref();
  const jsUrl = new URL(`${WASM_BUNDLE_BASENAME}.js`, base).toString();
  const wasmUrl = new URL(`${WASM_BUNDLE_BASENAME}_bg.wasm`, base).toString();

  try {
    const mod = await import(jsUrl);
    if (typeof mod.default !== "function") {
      throw new Error(`WASM bundle missing default init(): ${jsUrl}`);
    }
    await mod.default({ module_or_path: wasmUrl });
    wasmBindings = mod;
    state.ready = true;
    setStatus("Drop a SAVE.DAT file to render.", "info");
  } catch (error) {
    const message = extractErrorMessage(error);
    state.initError = message;
    console.error("WASM fallback init failed", { jsUrl, wasmUrl, error });
    setStatus(
      `Failed to initialize WASM module (fallback). ${message}`,
      "error",
    );
  } finally {
    wasmInitInProgress = false;
  }
}

async function copyOutput() {
  if (!state.renderedText) {
    return;
  }

  try {
    await navigator.clipboard.writeText(state.renderedText);
    setStatus("Output copied to clipboard.", "ok");
  } catch (_error) {
    setStatus("Clipboard access denied by the browser.", "error");
  }
}

function downloadOutput() {
  if (!state.renderedText) {
    return;
  }

  const blob = new Blob([state.renderedText], { type: "text/plain;charset=utf-8" });
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
    void renderFile(file);
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
    void renderFile(file);
  });
  elements.copyOutput.addEventListener("click", () => {
    void copyOutput();
  });
  elements.downloadOutput.addEventListener("click", downloadOutput);
}

async function main() {
  resetOutput();
  setStatus("Loading WASM parser...", "info");

  // Wire the UI immediately so clicks/drops give feedback while WASM loads.
  wireDragAndDrop();
  wireControls();

  // Trunk injects a bootstrap module that loads the wasm-bindgen bundle and then
  // dispatches "TrunkApplicationStarted" with bindings exposed on window.
  const onStarted = () => {
    const bindings = window.wasmBindings;
    if (!bindings || typeof bindings.render_save_text !== "function") {
      state.initError = "Missing wasm bindings (render_save_text)";
      setStatus(`Failed to initialize WASM module: ${state.initError}`, "error");
      return;
    }

    wasmBindings = bindings;
    state.ready = true;
    setStatus("Drop a SAVE.DAT file to render.", "info");
  };

  if (window.wasmBindings) {
    onStarted();
    return;
  }

  window.addEventListener(
    "TrunkApplicationStarted",
    () => {
      try {
        onStarted();
      } catch (error) {
        const message = extractErrorMessage(error);
        state.initError = message;
        console.error("Failed to initialize WASM module", error);
        setStatus(`Failed to initialize WASM module: ${message}`, "error");
      }
    },
    { once: true },
  );

  // If Trunk's bootstrap fails silently or never dispatches, try a direct load.
  wasmInitTimer = window.setTimeout(() => {
    if (!state.ready && !state.initError) {
      void initWasmFallback();
    }
  }, 2000);
}

void main();
