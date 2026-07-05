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
    const bar = document.getElementById("file-tabs");
    if (bar) {
      bar.innerHTML =
        `<span style="color:#f87171; font-size:12px; padding:0 12px">` +
        `Failed to load examples: ${err.message}` +
        `</span>`;
    }
  });

initCompiler();

document.getElementById("run-btn").addEventListener("click", run);
document.getElementById("theme-btn").addEventListener("click", toggleTheme);
document.getElementById("reset-btn").addEventListener("click", resetCurrentFile);
for (const btn of document.querySelectorAll(".tab-btn")) {
  btn.addEventListener("click", () => selectTab(btn.dataset.tab));
}
