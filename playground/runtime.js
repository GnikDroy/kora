export const EXTERNS = `
extern void clear();
extern void print(a: string);
extern string input();
extern void sleep(ms: int);
extern bool is_key_down(key: string);
extern real random();
extern void draw_clear();
extern void set_color(c: string);
extern void fill_rect(x: int, y: int, w: int, h: int);
extern void fill_circle(x: int, y: int, r: int);
extern void draw_text(s: string, x: int, y: int);
extern void stroke_rect(x: int, y: int, w: int, h: int);
extern void stroke_circle(x: int, y: int, r: int);
extern void draw_line(x1: int, y1: int, x2: int, y2: int);
extern void fill_triangle(x1: int, y1: int, x2: int, y2: int, x3: int, y3: int);
extern void set_line_width(w: int);
extern void set_font_size(px: int);
extern void set_alpha(a: real);
extern int canvas_width();
extern int canvas_height();
extern int text_width(s: string);
extern int mouse_x();
extern int mouse_y();
extern bool is_mouse_down();
extern void save();
extern void restore();
extern void translate(x: int, y: int);
extern void rotate(a: real);
extern real sqrt(x: real);
extern real sin(x: real);
extern real cos(x: real);
extern real atan2(y: real, x: real);
`;

// The subset of EXTERNS that blocks on a Promise.
export const ASYNC_EXTERNS = ["input", "sleep"];

export function clearOutput() {
  document.getElementById("stdout").innerText = "";
}

export function appendOutput(text) {
  document.getElementById("stdout").innerText += text;
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
