---
title: Hello World
description: Your first Kora program, explained line by line.
---

```ruby
import "std/io";

int main() {
    io.write("Hello, world!\n");
    return 0;
}
```

Paste it into the [playground](/kora/play/) and press **Run**, or save it as
`hello.kora` and compile it:

```sh
kora hello.kora -o hello && ./hello
```

Either way it prints `Hello, world!`.

## Line by line

- **`import "std/io";`** pulls in the standard I/O module. Modules are imported
  by path and named after the last segment, so its functions are `io.write`,
  `io.print`, and `io.input`.
- **`int main()`** is the entry point. Every program starts at `main`, which
  returns an `int` exit code. Return types come first: `int`, then the name.
- **`io.write("Hello, world!\n");`** writes a string to stdout with no trailing
  newline, hence the explicit `\n`. A `string` is an array of `char` (bytes).
  (`io.print` adds the newline for you.)
- **`return 0;`** exits with success.

## Reading input

`io.input` returns a `string?`, an optional, because input can end. Handle the
empty case before using the value:

```ruby
import "std/io";

int main() {
    io.write("What is your name? ");
    let name = io.input();
    if (name == none) { return 1; }
    io.write("Hello, ");
    io.write(name!);
    io.write("!\n");
    return 0;
}
```

The `!` force-unwraps the optional, safe here because the `none` case already
returned. In the playground, type into the **Standard input** box.

## Next

- [Kora in 5 Minutes](../kora-in-5-minutes/): the whole language in one pass.
- [Runtime Helpers](../../reference/runtime-helpers/): the standard library.
