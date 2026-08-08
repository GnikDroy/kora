---
title: Kora in 5 Minutes
description: "The whole language at a glance: types, control flow, structs, generics, optionals, and functions as values."
---

A quick tour of the whole language. If you know any C-like language, skim it.
Everything here runs in the [playground](/kora/play/).

## Comments and entry point

Line comments start with `#`. Programs start at `main`, which returns an `int`
exit code. A function's return type comes first, then its name and parameters.

```ruby
# a comment

int main() {
    return 0;
}
```

## Variables and types

`let` declares a variable. The type is inferred or written after a colon.

```ruby
let x = 42;          # int
let y: real = 3.14;
let name = "Kora";   # string
```

The primitives:

- `int`: integers. 64-bit natively, safe to 2^53 - 1 on the JavaScript backend.
- `real`: floating point.
- `char`: a single byte.
- `bool`: `true` or `false`.
- `string`: text, exactly an array of `char`.

Convert with `as`:

```ruby
let c = 65 as char;   # 'A'
let f = 65 as real;   # 65.0
```

At module level, `let` declares a constant. Assignment to it is a compile
error. Other modules reach it as `config.WIDTH`, like a function.

```ruby
let WIDTH = 640;
let AREA = WIDTH * 480;
let TITLE = "kora" + " v1";
```

## Arrays

`[...]` is a literal, and `new T[n]` makes `n` default-valued elements. Arrays
are bounds-checked and carry built-in methods.

```ruby
let xs = [1, 2, 3];
let zeros = new int[10];

xs.push(4);                # append
xs.insert(0, 9);           # insert at index
let last = xs.pop();       # remove and return last
xs.remove(0);              # remove by index
let mid = xs.slice(1, 3);  # sub-array [start, end)
xs.extend([7, 8]);         # append another array
let n = xs.len();
```

## Control flow

`if` / `else`, `while`, and C-style `for` behave as usual. `for x | array` is a
for-each that binds each element.

```ruby
for (let i = 0; i < 10; i = i + 1) {
    if (i % 2 == 0) { continue; }
    if (i > 7) { break; }
}

for x | [10, 20, 30] {
    io.write(conv.int_to_string(x));
}
```

## Functions

Return type, name, parameters, body. `void` means no return value.

```ruby
int add(a: int, b: int) { return a + b; }
void greet(name: string) { io.write(name); }
```

## Structs and methods

`struct` groups data, and `impl` attaches methods whose first parameter is
`self`. Construct with `new`.

```ruby
struct Point { x: int, y: int }

impl Point {
    int manhattan(self) {
        return math.abs(self.x) + math.abs(self.y);
    }
}

let p = new Point{ x: 3, y: -4 };
p.manhattan();   # 7
```

## Generics

Type parameters go in `<...>` on `struct`, `impl`, and functions. Instantiate
with `::<...>` or type arguments on `new`. Every instance is
monomorphized, so generics are a zero cost abstraction.

```ruby
struct pair<A, B> { first: A, second: B }

T id<T>(x: T) { return x; }

let p = new pair<int, string>{ first: id::<int>(1), second: "two" };
```

## Optionals

`T?` is a `T` or nothing. There is no null. Write `none` for empty, and
force-unwrap with `!` when you know a value is present (it panics if not).

```ruby
int? first_even(xs: [int]) {
    for x | xs {
        if (x % 2 == 0) { return x; }
    }
    return none;
}

let v = first_even([1, 3, 4, 7]);
if (v != none) { let n = v!; }
```

## Functions as values

Functions are first class. A function type is the return type followed by the
parameter types: `int(int, int)`. No closures.

```ruby
int apply(f: int(int), x: int) { return f(x); }
int square(n: int) { return n * n; }

let g: int(int) = square;
apply(g, 6);   # 36
```

## Modules

Split a program across files, imported by path. A module is named after its last
path segment unless renamed. The standard library is a set of modules written in
Kora.

```ruby
import "std/io";
import "std/math";
import "std/conv";

int main() {
    io.write(conv.int_to_string(math.gcd(48, 36)));   # 12
    return 0;
}
```

## Safety

Kora fails loudly. Out-of-bounds indexing, division by zero,
`pop()` on an empty array, and force-unwrapping `none` all panic with a clear
message. Garbage collection you never worry about freeing memory by hand.

## Next

- [Standard Library](../../reference/standard-library/): the full standard
  library, module by module.
- [Playground Functions](../../reference/playground-functions/): graphics and
  input for the browser.
