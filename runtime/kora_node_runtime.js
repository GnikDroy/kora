let stdoutBuffer = [];

function flushStdout() {
  if (stdoutBuffer.length === 0) return;
  require("node:fs").writeSync(1, Buffer.from(stdoutBuffer));
  stdoutBuffer = [];
}

function __kora_write(buf, n) {
  flushStdout();
  const bytes = Array.isArray(buf) ? buf.slice(0, Number(n)) : [];
  require("node:fs").writeSync(1, Buffer.from(bytes));
}

function putchar(c) {
  stdoutBuffer.push(Number(c) & 0xff);
  if (stdoutBuffer.length >= 4096) flushStdout();
  return c;
}

function getchar() {
  const buf = Buffer.alloc(1);
  try {
    return require("node:fs").readSync(0, buf, 0, 1) === 0 ? -1 : buf[0];
  } catch {
    return -1;
  }
}

function rand() {
  return Math.floor(Math.random() * 2147483648);
}

function __kora_sleep_ms(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, Number(ms));
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

function __kora_cstring_from_array(a) {
  return Array.isArray(a) ? Buffer.from(a).toString("utf8") : String(a);
}

function __kora_array_from_cstring(s) {
  return Array.from(Buffer.from(String(s), "utf8"));
}

function fopen(path, mode) {
  try {
    return { fd: require("node:fs").openSync(__kora_cstring_from_array(path), __kora_cstring_from_array(mode)), pos: 0 };
  } catch {
    return null;
  }
}

function fclose(f) {
  require("node:fs").closeSync(f.fd);
  return 0;
}

function fgetc(f) {
  const b = Buffer.alloc(1);
  if (require("node:fs").readSync(f.fd, b, 0, 1, f.pos) === 0) return -1;
  f.pos += 1;
  return b[0];
}

function fputc(c, f) {
  require("node:fs").writeSync(f.fd, Buffer.from([c]), 0, 1, f.pos);
  f.pos += 1;
  return c;
}

function fputs(s, f) {
  const b = Buffer.from(s);
  f.pos += require("node:fs").writeSync(f.fd, b, 0, b.length, f.pos);
  return 0;
}

function fflush(f) {
  if (f === null || f === undefined) flushStdout();
  return 0;
}

function ftell(f) {
  return f.pos;
}

function fseek(f, offset, whence) {
  const fs = require("node:fs");
  if (whence === 0) f.pos = Number(offset);
  else if (whence === 1) f.pos += Number(offset);
  else f.pos = fs.fstatSync(f.fd).size + Number(offset);
  return 0;
}

function remove(path) {
  try {
    require("node:fs").unlinkSync(__kora_cstring_from_array(path));
    return 0;
  } catch {
    return -1;
  }
}

function rename(from, to) {
  try {
    require("node:fs").renameSync(__kora_cstring_from_array(from), __kora_cstring_from_array(to));
    return 0;
  } catch {
    return -1;
  }
}

function getenv(name) {
  const v = process.env[__kora_cstring_from_array(name)];
  return v === undefined ? null : __kora_array_from_cstring(v);
}

function system(cmd) {
  const r = require("node:child_process").spawnSync(__kora_cstring_from_array(cmd), { shell: true, stdio: "inherit" });
  return r.status === null ? -256 : r.status * 256;
}

function exit(code) {
  process.exit(Number(code));
}

(async () => {
  process.exitCode = await __kora_main();
  flushStdout();
})();
