import init, { transpile } from "../pkg/compiler.js";
import {
  clearCompileErrors,
  editorReady,
  getSource,
  layoutJsView,
  setCompileErrors,
  setTranspiledJs,
} from "./editor.js";
import {
  ASYNC_EXTERNS,
  CANVAS_EXTERNS,
  appendOutput,
  clearOutput,
  fitTerminal,
  readLine,
} from "./runtime.js";

let compilerReady = false;
let isRunning = false;
let abortController = null;
let prettierPromise = null;

const PRETTIER_CDN = "https://cdn.jsdelivr.net/npm/prettier@3.9.4";

async function importPrettierFrom(base) {
  const [prettier, babel, estree] = await Promise.all([
    import(`${base}/standalone.mjs`),
    import(`${base}/plugins/babel.mjs`),
    import(`${base}/plugins/estree.mjs`),
  ]);
  return { prettier, plugins: [babel.default, estree.default] };
}

function loadPrettier() {
  if (!prettierPromise) {
    prettierPromise = importPrettierFrom(PRETTIER_CDN);
  }
  return prettierPromise;
}

async function prettify(code) {
  try {
    const { prettier, plugins } = await loadPrettier();
    return await prettier.format(code, { parser: "babel", plugins });
  } catch (err) {
    console.warn("prettier unavailable, showing raw output", err);
    return code;
  }
}

export async function initCompiler() {
  await init();
  compilerReady = true;
  setRunning(false);
}

export function selectTab(name) {
  for (const el of document.querySelectorAll(".tab-btn")) {
    el.classList.toggle("active", el.dataset.tab === name);
  }
  for (const el of document.querySelectorAll(".tab-panel")) {
    el.classList.toggle("hidden", el.dataset.tab !== name);
  }
  // Monaco and xterm need a re-layout when a hidden container becomes visible.
  if (name === "js") layoutJsView();
  if (name === "output") fitTerminal();
}

function setRunning(running) {
  isRunning = running;
  const btn = document.getElementById("run-btn");
  if (!btn) return;
  btn.disabled = false;
  btn.classList.remove("opacity-50", "cursor-not-allowed");
  btn.classList.toggle("bg-emerald-500", !running);
  btn.classList.toggle("hover:bg-emerald-600", !running);
  btn.classList.toggle("bg-red-500", running);
  btn.classList.toggle("hover:bg-red-600", running);
  btn.querySelector("i").className = running
    ? "fa-solid fa-stop text-xs"
    : "fa-solid fa-play text-xs";
  btn.querySelector(".run-label").textContent = running ? "Stop" : "Run";
}

// transferControlToOffscreen() taints an element permanently, so each run
// gets a fresh clone of the <canvas> to hand to its worker.
function freshCanvas() {
  const old = document.getElementById("canvas");
  if (!old) return null;
  const fresh = old.cloneNode(false);
  old.replaceWith(fresh);
  return fresh;
}

function executeInWorker(code, signal) {
  return new Promise((finish) => {
    const worker = new Worker(new URL("./worker.js", import.meta.url));
    const canvasEl = freshCanvas();
    const canvas = canvasEl ? canvasEl.transferControlToOffscreen() : null;

    function forwardKey(down) {
      return (e) => {
        if (e.target instanceof HTMLInputElement) return;
        worker.postMessage({ type: "key", key: e.key, down });
      };
    }
    const onKeyDown = forwardKey(true);
    const onKeyUp = forwardKey(false);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    // Map pointer position into the canvas's backing pixels (it is CSS-scaled).
    function onMouseMove(e) {
      const rect = canvasEl.getBoundingClientRect();
      worker.postMessage({
        type: "mouse",
        x: Math.round((e.clientX - rect.left) * (canvasEl.width / rect.width)),
        y: Math.round((e.clientY - rect.top) * (canvasEl.height / rect.height)),
      });
    }
    function onMouseDown() {
      worker.postMessage({ type: "mousebtn", down: true });
    }
    function onMouseUp() {
      worker.postMessage({ type: "mousebtn", down: false });
    }
    if (canvasEl) {
      canvasEl.addEventListener("mousemove", onMouseMove);
      canvasEl.addEventListener("mousedown", onMouseDown);
      window.addEventListener("mouseup", onMouseUp);
    }

    function done() {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      if (canvasEl) {
        canvasEl.removeEventListener("mousemove", onMouseMove);
        canvasEl.removeEventListener("mousedown", onMouseDown);
        window.removeEventListener("mouseup", onMouseUp);
      }
      worker.terminate();
      finish();
    }

    worker.onmessage = async (e) => {
      const msg = e.data;
      if (msg.type === "write") {
        appendOutput(msg.text);
      } else if (msg.type === "canvas") {
        selectTab("canvas");
      } else if (msg.type === "stdout") {
        selectTab("output");
      } else if (msg.type === "input") {
        try {
          const value = await readLine(signal);
          worker.postMessage({ type: "input-value", value });
        } catch {
          // Aborted; the abort listener below tears the worker down.
        }
      } else if (msg.type === "done") {
        done();
      } else if (msg.type === "error") {
        appendOutput("\n" + msg.message);
        done();
      }
    };
    worker.onerror = (e) => {
      appendOutput("\n" + e.message);
      done();
    };

    signal.addEventListener("abort", () => {
      appendOutput("\nStopped.");
      done();
    });

    worker.postMessage({ type: "run", code, canvas }, canvas ? [canvas] : []);
  });
}

export async function run() {
  // The guard is checked and set before any `await`, so we can
  // never start a second runner.
  if (isRunning) {
    if (abortController) abortController.abort();
    return;
  }
  setRunning(true);
  abortController = new AbortController();

  try {
    clearOutput();

    if (!compilerReady || !editorReady()) {
      appendOutput("Compiler is still loading…");
      return;
    }

    let compiled;
    try {
      compiled = transpile(getSource() + CANVAS_EXTERNS, ASYNC_EXTERNS);
    } catch (err) {
      setCompileErrors(String(err));
      appendOutput(String(err));
      setTranspiledJs("// Compile ERROR. see the editor and Output tab.");
      selectTab("output");
      return;
    }
    clearCompileErrors();

    setTranspiledJs(await prettify(compiled));
    await executeInWorker(compiled, abortController.signal);
  } finally {
    abortController = null;
    setRunning(false);
  }
}
