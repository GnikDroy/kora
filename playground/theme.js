import { setTerminalTheme } from "./runtime.js";

const STORAGE_KEY = "kora-theme";

export function currentTheme() {
  return localStorage.getItem(STORAGE_KEY) === "dark" ? "dark" : "light";
}

export function applyTheme(theme) {
  localStorage.setItem(STORAGE_KEY, theme);
  document.documentElement.classList.toggle("dark", theme === "dark");
  if (window.monaco) {
    monaco.editor.setTheme(theme === "dark" ? "kora-dark" : "kora-light");
  }
  setTerminalTheme(theme);
  const icon = document.querySelector("#theme-btn i");
  if (icon) {
    icon.className = theme === "dark" ? "fa-solid fa-sun" : "fa-solid fa-moon";
  }
}

export function toggleTheme() {
  applyTheme(currentTheme() === "dark" ? "light" : "dark");
}
