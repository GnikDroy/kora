importScripts("../runtime/kora_browser_runtime.js");

onmessage = async (e) => {
  const msg = e.data;
  if (msg.type === "run") {
    if (msg.canvas) ctx = msg.canvas.getContext("2d");
    try {
      const fn = new Function(
        ...Object.keys(KORA_EXTERNS),
        msg.code + "\nreturn __kora_main();",
      );
      await fn(...Object.values(KORA_EXTERNS));
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
