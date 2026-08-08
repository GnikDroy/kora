---
title: std/io
description: Standard input and output.
---

Standard input and output. Import with `import "std/io";`.

Everything here works on every backend except `is_tty`, which is native only.
In the playground, `input` reads from the **Standard input** box.

## Functions

### `write`

```ruby
void write(s: string)
```

Writes `s` to standard output. No newline is added.

```ruby
io.write("no newline");
io.write(", same line\n");
```

### `print`

```ruby
void print(s: string)
```

Writes `s` followed by a newline. Equivalent to `write(s)` then `write("\n")`.

```ruby
io.print("one line");
io.print("another");
```

### `ewrite`, `eprint`

```ruby
void ewrite(s: string)
void eprint(s: string)
```

The same as `write` and `print`, but to standard error.

```ruby
io.eprint("usage: greet <name>");
```

### `input`

```ruby
string? input()
```

Reads one line from standard input and returns it without the trailing
newline. Returns `none` at end of input. A final line with no newline is still
returned.

```ruby
let line = io.input();
while (line != none) {
    io.print("read: " + line!);
    line = io.input();
}
```

### `is_tty`

```ruby
bool is_tty()
```

Returns `true` if standard output is a terminal, `false` if it is redirected
to a file or pipe. Native backend only.

```ruby
if (io.is_tty()) {
    io.print("interactive session");
}
```
