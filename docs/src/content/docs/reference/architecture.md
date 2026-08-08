---
title: Architecture
description: "How the Kora compiler fits together: one frontend, a typed IR, and two backends."
---

Kora is a single frontend feeding two backends. Source is lexed and parsed per
module, assembled and import-resolved into one program, monomorphized, then
checked in three separate passes. Both backends lower the checked program to the
same typed IR, which is why the native binary and the JavaScript output produce
byte-identical results.

```
╔═══ COMPILER FRONTEND ══════════════════════════════════════════════════╗
║           ┌──────────────────────────────────────────────┐             ║
║           │ Source files  (.kora + imported modules)     │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐  per module ║
║           │ Lexer                             src/lexer  │ ◀───┐       ║
║           │ characters ──▶ tokens                        │     │       ║
║           └──────────────────────────────────────────────┘     │       ║
║                                  │                             │       ║
║                                  ▼                             │       ║
║           ┌──────────────────────────────────────────────┐     │       ║
║           │ Parser                           src/parser  │ ◀───┤       ║
║           │ tokens ──▶ AST                               │     │       ║
║           └──────────────────────────────────────────────┘     │       ║
║                                  │                             │       ║
║                                  ▼                             │       ║
║           ┌──────────────────────────────────────────────┐     │       ║
║           │ Loader + Import Resolver         src/loader  │ ────┘       ║
║           │ assembles parsed modules; resolves imports   │             ║
║           │ ──▶ LoadedProgram (module graph)             │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Generic Instantiator        src/instantiate  │             ║
║           │ monomorphize <T>; fill concrete              │             ║
║           │ structs / impls / calls                      │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Symbol Resolver                              │             ║
║           │ src/semantic_analyzer/symbol_resolver        │             ║
║           │ symbol-table pass; cross-module binding      │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Constant Evaluation                          │             ║
║           │ src/semantic_analyzer/const_eval             │             ║
║           │ fold module-level lets, declaration order    │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Type Checker                                 │             ║
║           │ src/semantic_analyzer/type_checker           │             ║
║           │ infer & check types; resolve methods         │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Return-Flow Analysis                         │             ║
║           │ src/semantic_analyzer  (ReturnChecker)       │             ║
║           │ every path returns a value                   │             ║
║           └──────────────────────────────────────────────┘             ║
║                                  │                                     ║
║                                  ▼                                     ║
║                                                                        ║
║               CompiledProgram   (checked AST + types)                  ║
║                                                                        ║
║                                  │                                     ║
║                                  ▼                                     ║
║           ┌──────────────────────────────────────────────┐             ║
║           │ Typed IR Lowering                    src/ir  │             ║
║           │ Name Mangling                    src/mangle  │             ║
║           │ monomorphic ops, finalize mangled symbols,   │             ║
║           │ explicit Wrap/Unwrap / Copy, lvalue detection│             ║
║           └──────────────────────────────────────────────┘             ║
║                       typed IR  (ir::Program)                          ║
╚════════════════════════════════════════════════════════════════════════╝
                                     │
                                     ▼
╔═══ COMPILER BACKEND ═══════════════════════════════════════════════════╗
║                                  │                                     ║
║                 ┌────────────────┴────────────────┐                    ║
║                 ▼                                 ▼                    ║
║  ┌────────────────────────────┐    ┌────────────────────────────┐      ║
║  │ JavaScript Transpiler      │    │ LLVM Lowering Pass         │      ║
║  │ src/javascript_transpiler  │    │ src/codegen  (inkwell)     │      ║
║  └────────────────────────────┘    │ emits an LLVM IR module    │      ║
║                 │                  └────────────────────────────┘      ║
║                 ▼                                 │                    ║
║  ┌────────────────────────────┐                   ▼                    ║
║  │ Async Coloring pass        │    ┌────────────────────────────┐      ║
║  │ marks fns that block on    │    │ Object File Emission       │      ║
║  │ an async extern as async   │    │ src/codegen (TargetMachine)│      ║
║  └────────────────────────────┘    │ LLVM IR ──▶ native .o      │      ║
║                 │                  └────────────────────────────┘      ║
║                 ▼                                 │                    ║
║  ┌────────────────────────────┐                   ▼                    ║
║  │ Emit JavaScript            │    ┌────────────────────────────┐      ║
║  │ + runtime inclusion:       │    │ Linking                    │      ║
║  │ kora_node_runtime.js /     │    │ src/codegen::link          │      ║
║  │ kora_browser_runtime.js    │    │ + libkora.c runtime        │      ║
║  └────────────────────────────┘    │ + Boehm GC + mbedTLS       │      ║
║                 │                  └────────────────────────────┘      ║
║         ┌───────┴───────────┐                     │                    ║
║         ▼                   ▼                     ▼                    ║
║  ┌─────────────┐ ┌────────────────────┐ ┌────────────────────┐         ║
║  │ Node.js     │ │ Playground         │ │ native executable  │         ║
║  │ (.js output)│ │ wasm compiler +    │ │ (LLVM 21 + linker) │         ║
║  └─────────────┘ │ browser runtime +  │ └────────────────────┘         ║
║                  │ canvas host worker │                                ║
║                  └────────────────────┘                                ║
╚════════════════════════════════════════════════════════════════════════╝
```
