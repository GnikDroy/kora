function __kora_write(s) {
  process.stdout.write(Array.isArray(s) ? s.join("") : String(s));
}

function __kora_getchar() {
  const buf = Buffer.alloc(1);
  try {
    return require("node:fs").readSync(0, buf, 0, 1) === 0 ? -1 : buf[0];
  } catch {
    return -1;
  }
}

function __kora_random() {
  return Math.random();
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, Number(ms));
}

(async () => {
  process.exitCode = await __kora_main();
})();
