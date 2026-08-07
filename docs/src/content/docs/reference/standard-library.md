---
title: Standard Library
description: An overview of every Kora standard library module, with conventions and availability.
---

The standard library is a set of modules written in Kora and shipped with the
compiler. Import a module by path. It is named after its last path segment
unless you rename it:

```ruby
import "std/math";      # math.abs(...), math.gcd(...)
import "std/math" m;    # renamed: m.abs(...)
```

## Modules

| Module | What it covers | Availability |
| --- | --- | --- |
| [std/io](../std/io/) | Standard input and output | everywhere |
| [std/conv](../std/conv/) | Converting values to and from strings | everywhere |
| [std/str](../std/str/) | String searching, splitting, and transforming | everywhere |
| [std/math](../std/math/) | Integer and floating-point math, random numbers | everywhere |
| [std/time](../std/time/) | Sleeping and reading clocks | everywhere |
| [std/iter](../std/iter/) | map / filter / reduce over arrays | everywhere |
| [std/algorithm](../std/algorithm/) | Generic sorting and binary search | everywhere |
| [std/collections](../std/collections/) | Stack, queue, list, set, and map | everywhere |
| [std/term](../std/term/) | ANSI terminal control | everywhere |
| [std/fs](../std/fs/) | Files and directories | native (files also on Node) |
| [std/env](../std/env/) | Environment variables and command-line args | native (`get` also on Node) |
| [std/proc](../std/proc/) | Running commands, exiting | native (`run`, `exit` also on Node) |
| [std/net](../std/net/) | TCP and UDP sockets | native |
| [std/thread](../std/thread/) | Threads, mutexes, condition variables | native |

"Everywhere" means the native backend, Node, and the browser playground. The
playground additionally provides
[graphics and input functions](../playground-functions/) that are not part of
the standard library.

## Conventions

The library reports failure through types, not exceptions:

- `T?` for absence or failure. Anything that can legitimately come up empty
  returns an optional: `io.input()` is `none` at end of input, `fs.open` is
  `none` for a missing file, `map.get` is `none` for an absent key. Check
  against `none`, then unwrap with `!`.
- `bool` for success. Operations where you only need to know whether it
  worked, such as `fs.mkdir`, `env.set`, and `socket.send_all`, return `true`
  on success.
- Panics for programmer errors. Popping an empty stack or indexing out of
  bounds stops the program with a message. These are bugs, not conditions to
  handle.

Generic functions and containers take explicit type arguments with the
turbofish: `iter.map::<int, string>(xs, f)`, `stack.make::<int>()`. Where an
ordering or a hash is needed, you supply it as a small struct: a comparator
with a `less` method for [std/algorithm](../std/algorithm/), or a hasher with a
`hash` method for [std/collections](../std/collections/).
