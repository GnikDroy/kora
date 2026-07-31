let pendingInput = null;
let stdoutShown = false;

function useStdout() {
  if (!stdoutShown) {
    stdoutShown = true;
    postMessage({ type: "stdout" });
  }
}

let stdoutBuffer = [];

function flushStdout() {
  if (stdoutBuffer.length === 0) return;
  useStdout();
  postMessage({ type: "write", text: stdoutBuffer.join("") });
  stdoutBuffer = [];
}

function __kora_write(buf, n) {
  flushStdout();
  useStdout();
  const text = Array.isArray(buf) ? buf.slice(0, Number(n)).join("") : String(buf);
  postMessage({ type: "write", text });
}

function putchar(c) {
  stdoutBuffer.push(String.fromCharCode(Number(c) & 0xff));
  if (stdoutBuffer.length >= 4096) flushStdout();
  return c;
}

function fflush(f) {
  if (f === null || f === undefined) flushStdout();
  return 0;
}

let inputBuffer = [];
async function getchar() {
  if (inputBuffer.length === 0) {
    useStdout();
    const line = await new Promise((resolve) => {
      pendingInput = resolve;
      postMessage({ type: "input" });
    });
    inputBuffer = line;
    inputBuffer.push("\n");
  }
  return inputBuffer.shift().charCodeAt(0);
}

function rand() {
  return Math.floor(Math.random() * 2147483648);
}

function __kora_sleep_ms(ms) {
  return new Promise((resolve) => setTimeout(resolve, Number(ms)));
}

function sqrt(x) { return Math.sqrt(x); }
function sin(x) { return Math.sin(x); }
function cos(x) { return Math.cos(x); }
function tan(x) { return Math.tan(x); }
function atan(x) { return Math.atan(x); }
function atan2(y, x) { return Math.atan2(y, x); }
function pow(base, exp) { return Math.pow(base, exp); }
function floor(x) { return Math.floor(x); }
function ceil(x) { return Math.ceil(x); }
function round(x) { return Math.sign(x) * Math.round(Math.abs(x)); }
function exp(x) { return Math.exp(x); }
function log(x) { return Math.log(x); }
function log2(x) { return Math.log2(x); }
function time() { return Math.floor(Date.now() / 1000); }
function log10(x) { return Math.log10(x); }
function asin(x) { return Math.asin(x); }
function acos(x) { return Math.acos(x); }
function sinh(x) { return Math.sinh(x); }
function cosh(x) { return Math.cosh(x); }
function tanh(x) { return Math.tanh(x); }
function hypot(x, y) { return Math.hypot(x, y); }
function cbrt(x) { return Math.cbrt(x); }
function trunc(x) { return Math.trunc(x); }
function fmod(x, y) { return x % y; }
function fabs(x) { return Math.abs(x); }
function fmin(x, y) { return Math.min(x, y); }
function fmax(x, y) { return Math.max(x, y); }

const KORA_EXTERNS = {
  __kora_write, putchar, fflush, getchar, rand, __kora_sleep_ms,
  sqrt, sin, cos, tan, atan, atan2, pow,
  floor, ceil, round, exp, log, log2, log10, time,
  asin, acos, sinh, cosh, tanh, hypot, cbrt, trunc, fmod, fabs, fmin, fmax,
};
