// Files shown as tabs above the editor. Order = tab order.
// Sources are fetched from res/ at page load; the same files are
// used by the Rust `transpiles_ui_examples` test.
const FILES = [
  "mandelbrot.kora",
  "sudoku.kora",
  "chess.kora",
  "snake.kora",
  "tetris.kora",
  "pong.kora",
  "doom.kora",
  "pacman.kora",
];

const models = {};
const originals = {};
let currentFile = FILES[0];
let editor = null;
let jsView = null;

export async function loadSources() {
  await Promise.all(FILES.map(async (name) => {
    const r = await fetch(`res/${name}`);
    if (!r.ok) throw new Error(`res/${name}: ${r.status} ${r.statusText}`);
    originals[name] = await r.text();
  }));
}

function updateScrollIndicators() {
  const bar = document.getElementById("file-tabs");
  const left = document.getElementById("tabs-left");
  const right = document.getElementById("tabs-right");
  if (!bar || !left || !right) return;
  left.classList.toggle("hidden", bar.scrollLeft <= 0);
  right.classList.toggle(
    "hidden",
    bar.scrollLeft + bar.clientWidth >= bar.scrollWidth - 1,
  );
}

export function createEditors(monaco) {
  const bar = document.getElementById("file-tabs");
  bar.addEventListener("wheel", (e) => {
    if (e.deltaY && !e.deltaX) {
      bar.scrollLeft += e.deltaY;
      e.preventDefault();
    }
  }, { passive: false });

  bar.addEventListener("scroll", updateScrollIndicators);
  new ResizeObserver(updateScrollIndicators).observe(bar);

  document.getElementById("tabs-left").addEventListener("click", () => {
    bar.scrollBy({ left: -bar.clientWidth * 0.6, behavior: "smooth" });
  });
  document.getElementById("tabs-right").addEventListener("click", () => {
    bar.scrollBy({ left: bar.clientWidth * 0.6, behavior: "smooth" });
  });

  bar.addEventListener("keydown", (e) => {
    const idx = FILES.indexOf(currentFile);
    let next = null;
    if (e.key === "ArrowRight") next = FILES[(idx + 1) % FILES.length];
    else if (e.key === "ArrowLeft") next = FILES[(idx - 1 + FILES.length) % FILES.length];
    else if (e.key === "Home") next = FILES[0];
    else if (e.key === "End") next = FILES[FILES.length - 1];
    if (next) {
      e.preventDefault();
      switchFile(next, { focusEditor: false });
    }
  });

  for (const name of FILES) {
    models[name] = monaco.editor.createModel(originals[name], "kora");
    models[name].onDidChangeContent(() => renderFileTabs());
  }

  editor = monaco.editor.create(document.getElementById("editor"), {
    model: models[currentFile],
    theme: "kora-light",
    automaticLayout: true,
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    padding: { top: 16 },
  });

  jsView = monaco.editor.create(document.getElementById("js-view"), {
    value: "// Transpiled JavaScript will appear here after you Run.",
    language: "javascript",
    theme: "kora-light",
    automaticLayout: true,
    readOnly: true,
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    padding: { top: 16 },
  });

  renderFileTabs();
}

export function renderFileTabs() {
  const bar = document.getElementById("file-tabs");
  if (!bar) return;
  bar.innerHTML = "";
  for (const name of FILES) {
    const btn = document.createElement("button");
    const active = name === currentFile;
    const dirty = models[name] && models[name].getValue() !== originals[name];
    btn.className = "file-tab" + (active ? " active" : "") + (dirty ? " dirty" : "");
    btn.title = dirty ? `${name} — edited, differs from the built-in example` : name;
    btn.setAttribute("role", "tab");
    btn.setAttribute("aria-selected", String(active));
    btn.tabIndex = active ? 0 : -1;
    btn.onclick = () => switchFile(name);
    const label = document.createElement("span");
    label.textContent = name.replace(/\.kora$/, "");
    const dot = document.createElement("span");
    dot.className = "dot";
    btn.appendChild(label);
    btn.appendChild(dot);
    bar.appendChild(btn);
    if (active) btn.scrollIntoView({ inline: "nearest", block: "nearest" });
  }
  updateScrollIndicators();
}

export function switchFile(name, { focusEditor = true } = {}) {
  if (!models[name]) return;
  currentFile = name;
  editor.setModel(models[name]);
  renderFileTabs();
  if (focusEditor) {
    editor.focus();
  } else {
    const active = document.querySelector("#file-tabs .file-tab.active");
    if (active) active.focus();
  }
}

export function resetCurrentFile() {
  const original = originals[currentFile];
  if (original == null) return;
  models[currentFile].setValue(original);
  renderFileTabs();
}

export function editorReady() {
  return editor !== null;
}

export function getSource() {
  return editor.getValue();
}

export function setTranspiledJs(text) {
  if (jsView) jsView.setValue(text);
}

export function layoutJsView() {
  if (jsView) jsView.layout();
}
