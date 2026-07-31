function write(fd, buf, n) {
  const text = Array.isArray(buf) ? buf.slice(0, Number(n)).join("") : String(buf);
  require("node:fs").writeSync(fd, Buffer.from(text, "binary"));
  return n;
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

function usleep(us) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, Number(us) / 1000);
}

(async () => {
  process.exitCode = await __kora_main();
})();
