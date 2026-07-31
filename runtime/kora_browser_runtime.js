let pendingInput = null;
let stdoutShown = false;

function useStdout() {
  if (!stdoutShown) {
    stdoutShown = true;
    postMessage({ type: "stdout" });
  }
}

function write(fd, buf, n) {
  useStdout();
  const text = Array.isArray(buf) ? buf.slice(0, Number(n)).join("") : String(buf);
  postMessage({ type: "write", text });
  return n;
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

function usleep(us) {
  return new Promise((resolve) => setTimeout(resolve, us / 1000));
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

function notProvided(name) {
  return () => {
    throw new Error(`extern '${name}' is not provided by the browser host`);
  };
}

const KORA_EXTERNS = {
  write, getchar, rand, usleep,
  sqrt, sin, cos, tan, atan, atan2, pow,
  floor, ceil, round, exp, log, log2, time,
  ...Object.fromEntries(
    ["fopen", "fclose", "fgetc", "fputc", "fputs", "fflush", "ftell", "fseek",
     "remove", "rename", "getenv", "system", "exit"]
      .map((n) => [n, notProvided(n)]),
  ),
};
