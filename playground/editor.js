const FILES = [
  "playground/mandelbrot.kora",
  "cli/sudoku.kora",
  "cli/calc.kora",
  "playground/snake.kora",
  "playground/tetris.kora",
  "playground/pong.kora",
  "playground/doom.kora",
  "playground/pacman.kora",
];

const models = {};
const originals = {};
let currentFile = FILES[0];
let editor = null;
let jsView = null;
let menuOpen = false;
let monacoRef = null;

const isDirty = (name) =>
  models[name] && models[name].getValue() !== originals[name];
const stem = (name) => name.replace(/^.*\//, "").replace(/\.kora$/, "");

export async function loadSources() {
  await Promise.all(FILES.map(async (name) => {
    const r = await fetch(`res/${name}`);
    if (!r.ok) throw new Error(`res/${name}: ${r.status} ${r.statusText}`);
    originals[name] = await r.text();
  }));
}

export function createEditors(monaco) {
  monacoRef = monaco;
  for (const name of FILES) {
    models[name] = monaco.editor.createModel(originals[name], "kora");
    models[name].onDidChangeContent(() => {
      renderTrigger();
      // Positions go stale as the user edits; drop them until the next Run.
      monaco.editor.setModelMarkers(models[name], "kora", []);
    });
  }

  const shared = {
    theme: "kora-light",
    automaticLayout: true,
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    padding: { top: 16 },
  };

  editor = monaco.editor.create(document.getElementById("editor"), {
    model: models[currentFile],
    ...shared,
  });

  jsView = monaco.editor.create(document.getElementById("js-view"), {
    value: "// Transpiled JavaScript will appear here after you Run.",
    language: "javascript",
    readOnly: true,
    ...shared,
  });

  setupFilePicker();
  renderTrigger();
}

function setupFilePicker() {
  const btn = document.getElementById("file-picker-btn");
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    toggleMenu();
  });
  btn.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openMenu();
      focusItem(FILES.indexOf(currentFile));
    }
  });
  document.getElementById("file-picker-menu").addEventListener(
    "keydown",
    onMenuKeydown,
  );
  document.addEventListener("click", closeMenu);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeMenu();
  });
}

function renderTrigger() {
  const label = document.getElementById("file-picker-label");
  if (!label) return;
  label.textContent = stem(currentFile) + (isDirty(currentFile) ? " •" : "");
}

function renderMenu() {
  const menu = document.getElementById("file-picker-menu");
  menu.innerHTML = "";
  for (const name of FILES) {
    const active = name === currentFile;
    const item = document.createElement("button");
    item.type = "button";
    item.className = "file-picker-item" + (active ? " active" : "");
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", String(active));
    item.tabIndex = -1;

    const text = document.createElement("span");
    text.className = "fp-name";
    text.textContent = stem(name);

    item.append(text);
    if (isDirty(name)) {
      const d = document.createElement("span");
      d.className = "fp-dirty";
      d.textContent = "•";
      d.title = "edited";
      item.append(d);
    }
    if (active) {
      const check = document.createElement("i");
      check.className = "fa-solid fa-check fp-check";
      item.append(check);
    }
    item.addEventListener("click", (e) => {
      e.stopPropagation();
      switchFile(name);
      closeMenu();
      editor.focus();
    });

    const li = document.createElement("li");
    li.append(item);
    menu.append(li);
  }
}

const menuItems = () =>
  Array.from(document.querySelectorAll("#file-picker-menu .file-picker-item"));

function focusItem(i) {
  const items = menuItems();
  if (!items.length) return;
  const idx = (i + items.length) % items.length;
  items.forEach((el) => el.classList.remove("focused"));
  items[idx].classList.add("focused");
  items[idx].focus();
}

function openMenu() {
  if (menuOpen) return;
  renderMenu();
  menuOpen = true;
  document.getElementById("file-picker-menu").classList.remove("hidden");
  document.getElementById("file-picker-btn").setAttribute("aria-expanded", "true");
}

function closeMenu() {
  if (!menuOpen) return;
  menuOpen = false;
  document.getElementById("file-picker-menu").classList.add("hidden");
  document.getElementById("file-picker-btn").setAttribute("aria-expanded", "false");
}

function toggleMenu() {
  if (menuOpen) {
    closeMenu();
  } else {
    openMenu();
    focusItem(FILES.indexOf(currentFile));
  }
}

function onMenuKeydown(e) {
  const items = menuItems();
  const cur = items.findIndex((el) => el.classList.contains("focused"));
  if (e.key === "ArrowDown") { e.preventDefault(); focusItem(cur + 1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); focusItem(cur - 1); }
  else if (e.key === "Home") { e.preventDefault(); focusItem(0); }
  else if (e.key === "End") { e.preventDefault(); focusItem(items.length - 1); }
  else if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    if (cur >= 0) items[cur].click();
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeMenu();
    document.getElementById("file-picker-btn").focus();
  }
}

export function switchFile(name, { focusEditor = true } = {}) {
  if (!models[name]) return;
  currentFile = name;
  editor.setModel(models[name]);
  renderTrigger();
  if (focusEditor) editor.focus();
}

export function resetCurrentFile() {
  const original = originals[currentFile];
  if (original == null) return;
  models[currentFile].setValue(original);
  renderTrigger();
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

// Every compiler error prints as `error: <msg> (<row>:<col>)`; position-less
// errors (e.g. at EOF) omit the parens and get no marker.
function parseErrors(text, model) {
  const markers = [];
  for (const line of text.split("\n")) {
    const m = line.match(/^error:\s*(.*?)\s*\((\d+):(\d+)\)\s*$/);
    if (!m) continue;
    const row = Math.min(Math.max(+m[2], 1), model.getLineCount());
    const col = +m[3];
    const word = model.getWordAtPosition({ lineNumber: row, column: col });
    markers.push({
      severity: monacoRef.MarkerSeverity.Error,
      message: m[1],
      startLineNumber: row,
      startColumn: word ? word.startColumn : col,
      endLineNumber: row,
      endColumn: word ? word.endColumn : col + 1,
    });
  }
  return markers;
}

export function setCompileErrors(text) {
  if (!monacoRef || !editor) return;
  const model = models[currentFile];
  monacoRef.editor.setModelMarkers(model, "kora", parseErrors(text, model));
}

export function clearCompileErrors() {
  if (!monacoRef) return;
  monacoRef.editor.setModelMarkers(models[currentFile], "kora", []);
}

export function layoutJsView() {
  if (jsView) jsView.layout();
}
