---
title: std/term
description: ANSI terminal control.
---

Minimal ANSI terminal control, built on escape sequences written to standard
output. Import with `import "std/term";`. The functions exist on every
backend, but they only have a visible effect when output goes to a terminal
that understands ANSI escapes.

## Functions

### `clear`

```ruby
void clear()
```

Clears the screen and moves the cursor to the top-left corner.

### `home`

```ruby
void home()
```

Moves the cursor to the top-left corner without clearing anything. Useful for
redrawing a screen in place:

```ruby
import "std/term";
import "std/time";

int main() {
    term.clear();
    while (true) {
        term.home();
        draw();            # rewrite the screen over the old frame
        time.sleep(100);
    }
    return 0;
}
```

### `csi`

```ruby
string csi(code: string)
```

Returns the ANSI control sequence for `code`: the escape byte, `[`, then
`code`. A building block for effects the module does not wrap.

```ruby
io.write(term.csi("1m"));    # bold
io.write(term.csi("0m"));    # reset
```
