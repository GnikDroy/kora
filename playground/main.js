import { registerKoraLanguage } from "./kora-language.js";
import { loadSources, createEditors, resetCurrentFile } from "./editor.js";
import { initCompiler, run, selectTab } from "./runner.js";
import { applyTheme, currentTheme, toggleTheme } from "./theme.js";

function monacoReady() {
  return new Promise((resolve) => require(["vs/editor/editor.main"], resolve));
}

applyTheme(currentTheme());

Promise.all([monacoReady(), loadSources()])
  .then(() => {
    registerKoraLanguage(monaco);
    createEditors(monaco);
    applyTheme(currentTheme());
  })
  .catch((err) => {
    console.error(err);
    const label = document.getElementById("file-picker-label");
    if (label) label.textContent = "Failed to load examples";
  });

initCompiler();

document.getElementById("run-btn").addEventListener("click", run);
document.getElementById("theme-btn").addEventListener("click", toggleTheme);
document.getElementById("reset-btn").addEventListener("click", resetCurrentFile);
for (const btn of document.querySelectorAll(".tab-btn")) {
  btn.addEventListener("click", () => selectTab(btn.dataset.tab));
}
