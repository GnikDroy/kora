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

const KORA_EXTERNS = { write, getchar, rand, usleep };
