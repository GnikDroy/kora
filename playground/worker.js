importScripts("../runtime/kora_browser_runtime.js");

const keysDown = new Set();
let ctx = null;
let canvasShown = false;
let mouseX = 0;
let mouseY = 0;
let mouseDown = false;

function useCanvas() {
  if (!canvasShown) {
    canvasShown = true;
    postMessage({ type: "canvas" });
  }
}

function useStdout() {
  if (!stdoutShown) {
    stdoutShown = true;
    postMessage({ type: "stdout" });
  }
}

function draw_clear() {
  if (!ctx) return;
  useCanvas();
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
}

function set_color(c) {
  if (!ctx) return;
  if (Array.isArray(c)) c = c.join("");
  ctx.fillStyle = c;
  ctx.strokeStyle = c;
}

function set_line_width(w) {
  if (!ctx) return;
  ctx.lineWidth = w;
}

function set_font_size(px) {
  if (!ctx) return;
  ctx.font = px + "px sans-serif";
}

function set_alpha(a) {
  if (!ctx) return;
  ctx.globalAlpha = a;
}

function stroke_rect(x, y, w, h) {
  if (!ctx) return;
  useCanvas();
  ctx.strokeRect(x, y, w, h);
}

function stroke_circle(x, y, r) {
  if (!ctx) return;
  useCanvas();
  ctx.beginPath();
  ctx.arc(x, y, r, 0, Math.PI * 2);
  ctx.stroke();
}

function draw_line(x1, y1, x2, y2) {
  if (!ctx) return;
  useCanvas();
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();
}

function fill_triangle(x1, y1, x2, y2, x3, y3) {
  if (!ctx) return;
  useCanvas();
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.lineTo(x3, y3);
  ctx.closePath();
  ctx.fill();
}

function canvas_width() {
  return ctx ? ctx.canvas.width : 0;
}

function canvas_height() {
  return ctx ? ctx.canvas.height : 0;
}

function text_width(s) {
  if (!ctx) return 0;
  if (Array.isArray(s)) s = s.join("");
  return Math.round(ctx.measureText(String(s)).width);
}

function mouse_x() {
  return mouseX;
}

function mouse_y() {
  return mouseY;
}

function is_mouse_down() {
  return mouseDown;
}

function save() {
  if (ctx) ctx.save();
}

function restore() {
  if (ctx) ctx.restore();
}

function translate(x, y) {
  if (ctx) ctx.translate(x, y);
}

function rotate(a) {
  if (ctx) ctx.rotate(a);
}

function fill_rect(x, y, w, h) {
  if (!ctx) return;
  useCanvas();
  ctx.fillRect(x, y, w, h);
}

function fill_circle(x, y, r) {
  if (!ctx) return;
  useCanvas();
  ctx.beginPath();
  ctx.arc(x, y, r, 0, Math.PI * 2);
  ctx.fill();
}

function draw_text(s, x, y) {
  if (!ctx) return;
  useCanvas();
  if (Array.isArray(s)) s = s.join("");
  ctx.fillText(String(s), x, y);
}

function is_key_down(key) {
  if (Array.isArray(key)) key = key.join("");
  return keysDown.has(key);
}

const CANVAS_IMPLS = {
  is_key_down,
  draw_clear, set_color, fill_rect, fill_circle, draw_text,
  stroke_rect, stroke_circle, draw_line, fill_triangle,
  set_line_width, set_font_size, set_alpha,
  canvas_width, canvas_height, text_width,
  mouse_x, mouse_y, is_mouse_down,
  save, restore, translate, rotate,
};
const EXTERN_IMPLS = { ...KORA_EXTERNS, ...CANVAS_IMPLS };

onmessage = async (e) => {
  const msg = e.data;
  if (msg.type === "run") {
    if (msg.canvas) ctx = msg.canvas.getContext("2d");
    try {
      const fn = new Function(
        ...Object.keys(EXTERN_IMPLS),
        msg.code + "\nreturn __kora_main();",
      );
      await fn(...Object.values(EXTERN_IMPLS));
      postMessage({ type: "done" });
    } catch (err) {
      postMessage({ type: "error", message: String(err) });
    }
  } else if (msg.type === "input-value" && pendingInput) {
    const resolve = pendingInput;
    pendingInput = null;
    resolve(Array.from(msg.value));
  } else if (msg.type === "key") {
    if (msg.down) keysDown.add(msg.key);
    else keysDown.delete(msg.key);
  } else if (msg.type === "mouse") {
    mouseX = msg.x;
    mouseY = msg.y;
  } else if (msg.type === "mousebtn") {
    mouseDown = msg.down;
  }
};
