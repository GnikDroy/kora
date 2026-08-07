---
title: std/term
description: ANSI terminal control, raw mode, and key input.
---

Terminal control: ANSI escapes, cursor movement, raw mode, and unbuffered key
input. Import with `import "std/term";`.

The escape helpers (`clear`, `home`, `move_to`, `cursor`, `csi`) exist on
every backend and take effect wherever output goes to an ANSI-capable
terminal. Raw mode and key input (`raw`, `read_key`, `cols`, `rows`) are
native only.

## Screen and cursor

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
redrawing a screen in place.

### `move_to`

```ruby
void move_to(row: int, col: int)
```

Moves the cursor to the given position. Rows and columns are 1-based, so
`move_to(1, 1)` is the top-left corner.

### `cursor`

```ruby
void cursor(visible: bool)
```

Shows or hides the cursor. Hide it while redrawing frames, and show it again
before exiting.

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

## Raw mode and keys

Native only. In raw mode, key presses reach the program immediately, without
echo and without waiting for Enter. Arrow keys and similar arrive as ANSI
escape sequences (`ESC [ A` for up, and so on) on every platform, including
Windows.

### `raw`

```ruby
bool raw(on: bool)
```

Enters or leaves raw mode. Returns `true` on success and `false` when standard
input is not a terminal. The original terminal state is restored automatically
when the program exits, even on a panic.

### `read_key`

```ruby
char? read_key(timeout_ms: int)
```

Returns the next input byte, waiting up to `timeout_ms` milliseconds. A
negative timeout waits forever. Returns `none` when no key arrived in time or
input has ended. Works on redirected input too, where it reads bytes from the
pipe or file.

### `cols`, `rows`

```ruby
int? cols()
int? rows()
```

The terminal size in character cells, or `none` when output is not a
terminal.

## Example: reading keys

```ruby
import "std/term";
import "std/io";

int main() {
    if (!term.raw(true)) {
        io.print("not a terminal");
        return 1;
    }
    io.print("press keys, q to quit");
    while (true) {
        let k = term.read_key(-1);
        if (k == none || k! == 'q') {
            break;
        }
        io.write("key: ");
        io.write([k!]);
        io.write("\n");
    }
    term.raw(false);
    return 0;
}
```

For frame-based drawing (editors, roguelikes, animations), combine `raw` with
`cursor(false)`, redraw with `home` or `move_to`, and pace the loop with
`read_key` timeouts or `time.sleep`.
