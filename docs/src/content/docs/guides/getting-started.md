---
title: Getting Started
description: Build the Kora compiler and run your first program.
---

Kora is a small, statically typed language with a garbage collector. It has three
runtimes from one frontend: an LLVM backend that lowers to native
executables, a JavaScript backend for Node, and the browser playground with extra canvas runtime.
Every runtime produces the same output.

To try Kora with nothing installed, [open the playground](/kora/play/). To build
native binaries, read on.

## Build the compiler

You need **Rust** (stable), **LLVM 22**, and a **C compiler** for the linker.

```sh
git clone https://github.com/gnikdroy/kora.git
cd kora
cargo build --release
```

This produces the `kora` compiler at `target/release/kora`. If the build cannot
find LLVM, point `LLVM_SYS_221_PREFIX` at your LLVM 22 install.

## Compile a program

```sh
kora program.kora -o program   # native binary
./program

kora program.kora --emit-js     # or print JavaScript instead
```

## Build the playground

The playground is the compiler built for WebAssembly (frontend and JavaScript
backend only, since LLVM cannot target wasm). It needs
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/).

```sh
wasm-pack build --target web --out-name compiler -- --no-default-features
python -m http.server
```

## Next

- [Hello World](../hello-world/): your first program, line by line.
- [Kora in 5 Minutes](../kora-in-5-minutes/): the whole language at a glance.
- [Runtime Helpers](../../reference/runtime-helpers/): the standard library.
- [Playground Functions](../../reference/playground-functions/): graphics and
  input for the browser.
