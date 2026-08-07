---
title: std/env
description: Environment variables and command-line arguments.
---

Environment variables and command-line arguments. Import with
`import "std/env";`.

`get` works on the native backend and Node. `args`, `set`, and `unset` are
native only.

## Functions

### `get`

```ruby
string? get(name: string)
```

Returns the value of the environment variable `name`, or `none` if it is not
set.

```ruby
let home = env.get("HOME");
if (home != none) {
    io.print(home!);
}
```

### `set`

```ruby
bool set(name: string, v: string)
```

Defines or overwrites the environment variable `name` for this process and its
children. Returns `true` on success.

### `unset`

```ruby
bool unset(name: string)
```

Removes the environment variable `name`. Returns `true` on success.

```ruby
env.set("MODE", "debug");
env.get("MODE");            # "debug"
env.unset("MODE");
env.get("MODE");            # none
```

### `args`

```ruby
[string] args()
```

Returns the command-line arguments. `args()[0]` is the program itself, so real
arguments start at index 1.

```ruby
import "std/env";
import "std/io";

int main() {
    let argv = env.args();
    if (argv.len() < 2) {
        io.print("usage: greet <name>");
        return 1;
    }
    io.print("Hello, " + argv[1]);
    return 0;
}
```
