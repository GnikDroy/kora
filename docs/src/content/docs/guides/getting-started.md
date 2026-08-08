---
title: Getting Started
description: A gentle introduction to Kora. Run your first program, then tour the language one feature at a time.
---

Kora is a small, statically typed programming language. If you have written any
C-like language you will feel at home. Programs are made of functions, structs,
and modules, with a few modern conveniences layered on top. Types are inferred,
generics are monomorphized, optionals replace null, and a garbage collector
means you never free memory by hand.

One compiler serves three runtimes. The LLVM backend produces native
executables, the JavaScript backend produces programs for Node, and the browser
playground runs the compiler itself as WebAssembly with an extra canvas API for
graphics. A program behaves the same on all three.

Here is what Kora looks like:

```ruby
import "std/io";
import "std/conv";

int fib(n: int) {
    if (n < 2) { return n; }
    return fib(n - 1) + fib(n - 2);
}

int main() {
    for (let i = 0; i < 10; i = i + 1) {
        io.print(conv.int_to_string(fib(i)));
    }
    return 0;
}
```

This guide starts from nothing and works up through the language one feature at
a time. If you would rather see everything on one page, read
[Kora in 5 Minutes](../kora-in-5-minutes/).

## Try Kora in the browser

The fastest way to try Kora is the [playground](/kora/play/): nothing to
install, and the file menu has example programs ranging from a word counter up
to Snake, Tetris, and Doom. The playground also adds
[drawing and input functions](../../reference/playground-functions/) for
building small games.

## Install the toolchain

To build native executables you need the compiler. Building it requires
Rust (stable), LLVM 21, and a C compiler for linking.

```sh
git clone https://github.com/gnikdroy/kora.git
cd kora
cargo build --release
```

This produces the `kora` compiler at `target/release/kora`. If the build cannot
find LLVM, point `LLVM_SYS_211_PREFIX` at your LLVM 21 installation.

## Compile and run

`kora` takes a source file and produces a native executable (named after the
input unless you pass `-o`):

```sh
kora hello.kora -o hello
./hello
```

Pass `--emit-js` to produce JavaScript instead. It writes `hello.js` next to
the input, ready to run with Node:

```sh
kora hello.kora --emit-js
node hello.js
```

Optimization levels are set with `-O` (`0` to `3`, `s`, `z`, default `2`).
Anything after `--` is passed to the linker.

If you want to hack on the playground itself, it is the same compiler built
for WebAssembly (frontend and JavaScript backend only, since LLVM cannot
target wasm). It needs [`wasm-pack`](https://rustwasm.github.io/wasm-pack/):

```sh
wasm-pack build --target web --out-name compiler -- --no-default-features
python -m http.server
```

## Your first program

```ruby
import "std/io";

int main() {
    io.write("Hello, world!\n");
    return 0;
}
```

Every program starts at `main`, which returns an `int` exit code, `0` meaning
success. Notice that the return type comes *before* the function name. That is
true of every Kora function. `import "std/io";` pulls in the standard I/O
module, whose functions are then called as `io.write`, `io.print`, and so on.

For a slower walk through this program, see [Hello World](../hello-world/).

## Variables and inference

`let` declares a variable. The compiler infers the type from the initializer,
or you can write it yourself after a colon:

```ruby
let x = 42;            # inferred as int
let y: real = 3.14;    # annotated
let name = "Kora";     # string

x = x + 1;             # plain assignment; x stays an int
```

Types are checked at compile time: assigning a `string` to `x` is an error, not
a surprise at runtime.

## Constants

A `let` at module level, outside any function, declares a constant. The
initializer is any expression built from literals, constants declared above
it, and operators, evaluated at compile time. Assigning to the name is a
compile error. Division by zero is also a compile error.

```ruby
let WIDTH = 640;
let HEIGHT = 480;
let AREA = WIDTH * HEIGHT;
let TITLE = "kora" + " v1";
let ESC = 27 as char;

int rows(cell: int) {
    return HEIGHT / cell;
}
```

A constant is visible anywhere in its module, and other files reach it through
the module name after an import, just like a function: `config.WIDTH`. A
constant simply names its computed value, so every use is that value. Locals
may shadow it.

## The basic types

Kora has five primitive types:

- `int`: integers. 64-bit on the native backend, and safe up to 2^53 - 1 on
  the JavaScript backend.
- `real`: 64-bit floating point.
- `char`: a single byte, written `'a'`, `'\n'`, `'\t'`.
- `bool`: `true` or `false`.
- `string`: text. A `string` is exactly an array of `char` (bytes), so
  everything arrays can do, strings can do.

There are no implicit conversions. Convert explicitly with `as`:

```ruby
let c = 65 as char;     # 'A'
let f = 65 as real;     # 65.0
let n = 3.9 as int;     # 3   (the fraction is dropped)
```

Arithmetic (`+ - * / %`), comparisons (`== != < <= > >=`), and logic
(`&& || !`) work as in C. On strings and arrays, `==` compares contents,
element by element. `+` concatenates strings:

```ruby
let greeting = "Hello, " + "Kora";
```

## Control flow

`if`/`else`, `while`, and the C-style `for` behave as you expect, with `break`
and `continue` available in loops. Conditions must be `bool`. There is no
truthiness.

```ruby
if (x % 2 == 0) {
    io.print("even");
} else {
    io.print("odd");
}

while (x > 0) {
    x = x - 1;
}

for (let i = 0; i < 10; i = i + 1) {
    if (i == 3) { continue; }
    if (i == 7) { break; }
}
```

There is also a for-each form, `for x | array`, which binds each element in
turn:

```ruby
for word | ["one", "two", "three"] {
    io.print(word);
}
```

## Functions

A function is written return type first, then the name, then parameters as
`name: type`. Use `void` for no return value.

```ruby
int add(a: int, b: int) {
    return a + b;
}

void greet(name: string) {
    io.print("Hello, " + name);
}
```

Functions are also values. A function *type* is written as the return type
followed by the parameter types in parentheses, so `int(int)` reads as
"a function taking an `int` and returning an `int`":

```ruby
int square(n: int) { return n * n; }

int apply(f: int(int), x: int) { return f(x); }

let g: int(int) = square;
apply(g, 6);    # 36
```

Passing behavior around like this is how the [`std/iter`](../../reference/std/iter/)
helpers work. Kora has no closures yet. You pass named, top-level functions.

## Arrays

`[...]` is an array literal, and `new T[n]` makes an array of `n` default-valued
elements. Arrays grow dynamically and carry built-in methods:

```ruby
let xs = [1, 2, 3];
let zeros = new int[10];

xs.push(4);                 # [1, 2, 3, 4]
xs.insert(0, 9);            # [9, 1, 2, 3, 4]
let last = xs.pop();        # 4; removes it
xs.remove(0);               # removes index 0
let mid = xs.slice(1, 3);   # elements [1, 3), a new array
xs.extend([7, 8]);          # append every element of another array
let n = xs.len();
```

Every index is bounds-checked: `xs[99]` on a three-element array stops the
program with a clear message rather than reading garbage. And since a `string`
is an array of `char`, all of the above works on strings too: `s.len()`,
`s[0]`, `s.slice(1, 3)`.

## Structs and methods

`struct` groups named fields. Construct values with `new`, and attach methods
in an `impl` block, where each method's first parameter is `self`.

```ruby
struct Point {
    x: int,
    y: int,
}

impl Point {
    int manhattan(self) {
        return math.abs(self.x) + math.abs(self.y);
    }

    void move_by(self, dx: int, dy: int) {
        self.x = self.x + dx;
        self.y = self.y + dy;
    }
}

let p = new Point{ x: 3, y: -4 };
p.move_by(1, 1);
p.manhattan();    # 7
```

Structs and arrays are reference types. Assigning one to a new variable or
passing it to a function shares the same underlying value, which is why
`move_by` can update the caller's point. The primitives (`int`, `real`, `char`,
`bool`) are copied by value.

## Optionals

There is no null in Kora. When a value can be absent, its type says so: `T?` is
"a `T`, or nothing". The empty value is written `none`, and you cannot use a
`T?` where a `T` is expected without handling the empty case first.

```ruby
int? first_even(xs: [int]) {
    for x | xs {
        if (x % 2 == 0) { return x; }
    }
    return none;
}

let found = first_even([1, 3, 4, 7]);
if (found != none) {
    let n = found!;    # unwrap: found is known to be present
}
```

The `!` operator force-unwraps an optional. If the value is `none`, the program
panics. So unwrap only after checking, or when absence would be a bug anyway.
Much of the standard library returns optionals for things that can legitimately
fail: `io.input()` at end of input, `conv.string_to_int` on a malformed string,
`fs.open` on a missing file.

## Generics

Structs, impls, and functions take type parameters in `<...>`. Supply type
arguments with `::<...>` (the "turbofish") on calls, or directly on `new`:

```ruby
struct pair<A, B> {
    first: A,
    second: B,
}

T id<T>(x: T) { return x; }

let p = new pair<int, string>{ first: id::<int>(1), second: "two" };
```

Every instantiation is monomorphized. The compiler generates concrete code per
type, so generics cost nothing at runtime. The generic containers in
[`std/collections`](../../reference/std/collections/) are built this way.

## Modules

Programs split across files. `import` takes a path. The module is named after
the last path segment, or a name you choose:

```ruby
import "std/math";        # used as math.abs(...)
import "std/math" m;      # or renamed: m.abs(...)
import "geometry.kora";   # your own file, used as geometry.area(...)
```

The [standard library](../../reference/standard-library/) is itself a set of
Kora modules covering I/O, strings, math, files, networking, threads,
collections, and more. Import what you need. Nothing is loaded implicitly.

## When things go wrong

Kora fails loudly, not silently. Indexing out of bounds, dividing by zero,
`pop()` on an empty array, and force-unwrapping `none` all stop the program
with a clear message. There is no undefined behavior and no corrupted memory.
The garbage collector reclaims unused structs and arrays automatically, so
there is no `free` and no use-after-free.

## Where to go next

- [Kora in 5 Minutes](../kora-in-5-minutes/): the whole language on one page.
- [Standard Library](../../reference/standard-library/): every module, with
  signatures and examples.
- [Playground Functions](../../reference/playground-functions/): graphics and
  input for browser games.
- [Architecture](../../reference/architecture/): how the compiler itself is
  put together.
