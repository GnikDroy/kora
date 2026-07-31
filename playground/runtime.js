export const CANVAS_EXTERNS = `
extern bool is_key_down(key: cstring);
extern void draw_clear();
extern void set_color(c: cstring);
extern void fill_rect(x: int64, y: int64, w: int64, h: int64);
extern void fill_circle(x: int64, y: int64, r: int64);
extern void draw_text(s: cstring, x: int64, y: int64);
extern void stroke_rect(x: int64, y: int64, w: int64, h: int64);
extern void stroke_circle(x: int64, y: int64, r: int64);
extern void draw_line(x1: int64, y1: int64, x2: int64, y2: int64);
extern void fill_triangle(x1: int64, y1: int64, x2: int64, y2: int64, x3: int64, y3: int64);
extern void set_line_width(w: int64);
extern void set_font_size(px: int64);
extern void set_alpha(a: float64);
extern int64 canvas_width();
extern int64 canvas_height();
extern int64 text_width(s: cstring);
extern int64 mouse_x();
extern int64 mouse_y();
extern bool is_mouse_down();
extern void save();
extern void restore();
extern void translate(x: int64, y: int64);
extern void rotate(a: float64);
`;

// The subset of EXTERNS that blocks on a Promise.
export const ASYNC_EXTERNS = ["__kora_getchar", "sleep"];

const TERMINAL_THEMES = {
  light: {
    background: "#F9F4E4",
    foreground: "#3C3836",
    cursor: "#9D0006",
    selectionBackground: "#EBDBB2",
    black: "#FBF1C7",
    red: "#CC241D",
    green: "#98971A",
    yellow: "#D79921",
    blue: "#458588",
    magenta: "#B16286",
    cyan: "#689D6A",
    white: "#7C6F64",
    brightBlack: "#928374",
    brightRed: "#9D0006",
    brightGreen: "#79740E",
    brightYellow: "#B57614",
    brightBlue: "#076678",
    brightMagenta: "#8F3F71",
    brightCyan: "#427B58",
    brightWhite: "#3C3836",
  },
  dark: {
    background: "#1D2021",
    foreground: "#EBDBB2",
    cursor: "#FB4934",
    selectionBackground: "#504945",
    black: "#1D2021",
    red: "#CC241D",
    green: "#98971A",
    yellow: "#D79921",
    blue: "#458588",
    magenta: "#B16286",
    cyan: "#689D6A",
    white: "#A89984",
    brightBlack: "#928374",
    brightRed: "#FB4934",
    brightGreen: "#B8BB26",
    brightYellow: "#FABD2F",
    brightBlue: "#83A598",
    brightMagenta: "#D3869B",
    brightCyan: "#8EC07C",
    brightWhite: "#EBDBB2",
  },
};

let term = null; // created lazily on first output
let fitAddon = null;
let themeName = "light";

function terminal() {
  if (!term) {
    term = new Terminal({
      convertEol: true,
      fontSize: 13,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
      cursorBlink: false,
      disableStdin: true,
      theme: TERMINAL_THEMES[themeName],
    });
    fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(document.getElementById("terminal"));
    fitAddon.fit();
    window.addEventListener("resize", () => fitAddon.fit());
  }
  return term;
}

export function setTerminalTheme(theme) {
  themeName = TERMINAL_THEMES[theme] ? theme : "light";
  if (term) {
    // A fresh object per assignment: xterm's options service skips updates
    // when it considers the value unchanged.
    term.options.theme = { ...TERMINAL_THEMES[themeName] };
    term.refresh(0, term.rows - 1);
  }
}

export function fitTerminal() {
  if (fitAddon) fitAddon.fit();
}

export function clearOutput() {
  terminal().reset();
}

export function appendOutput(text) {
  terminal().write(text);
}

export function readLine(signal) {
  const el = document.getElementById("stdin");
  const prevPlaceholder = el.placeholder;
  el.value = "";
  el.placeholder = "Type input, then press Enter…";
  el.classList.add("kora-awaiting-input");
  el.focus();

  return new Promise((resolve, reject) => {
    function cleanup() {
      el.value = "";
      el.placeholder = prevPlaceholder;
      el.classList.remove("kora-awaiting-input");
      el.removeEventListener("keydown", onKey);
      signal.removeEventListener("abort", onAbort);
    }
    function onKey(e) {
      if (e.key !== "Enter") return;
      e.preventDefault();
      const value = el.value;
      cleanup();
      appendOutput(value + "\n");
      resolve(value);
    }
    function onAbort() {
      cleanup();
      reject(signal.reason);
    }
    el.addEventListener("keydown", onKey);
    signal.addEventListener("abort", onAbort);
  });
}
