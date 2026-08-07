---
title: std/fs
description: Files and directories.
---

Files and directories. Import with `import "std/fs";`.

The full module works on the native backend. On Node, `open`, the `File`
methods, `remove`, and `rename` also work, but the directory and metadata
helpers are native only. Nothing here is available in the playground.

## Opening files

### `open`

```ruby
File? open(path: string, mode: string)
```

Opens the file at `path` and returns a `File` handle, or `none` on failure
(missing file, no permission). `mode` is a C `fopen` mode:

| Mode | Meaning |
| --- | --- |
| `"r"` | read, the file must exist |
| `"w"` | write, truncates or creates |
| `"a"` | append, creates if missing |
| `"r+"` | read and write, the file must exist |
| `"w+"` | read and write, truncates or creates |
| `"a+"` | read and append, creates if missing |

Call `close` on the handle when done.

```ruby
import "std/fs";
import "std/io";

int main() {
    let opened = fs.open("notes.txt", "w");
    if (opened == none) {
        io.print("cannot open notes.txt");
        return 1;
    }
    let f = opened!;
    f.write("first line\n");
    f.close();
    return 0;
}
```

## Methods on `File`

### `read_char`

```ruby
char? read_char(self)
```

Reads and returns the next byte, or `none` at end of file.

### `read_line`

```ruby
string? read_line(self)
```

Reads one line, without the trailing newline. Returns `none` at end of file.

```ruby
let line = f.read_line();
while (line != none) {
    io.print(line!);
    line = f.read_line();
}
```

### `read_all`

```ruby
string read_all(self)
```

Reads everything from the current position to the end of the file.

### `write`

```ruby
void write(self, s: string)
```

Writes `s` at the current position.

### `flush`

```ruby
void flush(self)
```

Forces buffered writes out to the file.

### `tell`

```ruby
int tell(self)
```

Returns the current position, in bytes from the start of the file.

### `seek`

```ruby
void seek(self, offset: int)
```

Moves the current position to `offset` bytes from the start of the file.

### `close`

```ruby
void close(self)
```

Closes the handle. Do not use it afterwards.

## Manipulating paths

### `exists`

```ruby
bool exists(path: string)
```

Returns `true` if `path` exists (file or directory).

### `is_dir`

```ruby
bool is_dir(path: string)
```

Returns `true` if `path` exists and is a directory.

### `size`

```ruby
int? size(path: string)
```

Returns the file's size in bytes, or `none` if it does not exist.

### `mtime`

```ruby
int? mtime(path: string)
```

Returns the last-modified time as Unix seconds, or `none` if the path does not
exist.

### `remove`

```ruby
bool remove(path: string)
```

Deletes the file at `path`. Returns `true` on success.

### `rename`

```ruby
bool rename(from: string, to: string)
```

Renames (moves) `from` to `to`. Returns `true` on success.

### `chmod`

```ruby
bool chmod(path: string, mode: int)
```

Sets Unix permission bits on `path`, for example `493` for `rwxr-xr-x`.
Returns `true` on success.

## Directories

### `mkdir`

```ruby
bool mkdir(path: string)
```

Creates a directory. Returns `true` on success (the parent must exist).

### `rmdir`

```ruby
bool rmdir(path: string)
```

Removes an empty directory. Returns `true` on success.

### `read_dir`

```ruby
[string] read_dir(path: string)
```

Returns the entry names in the directory, excluding `.` and `..`. Returns an
empty array if the directory cannot be read.

```ruby
for name | fs.read_dir(".") {
    io.print(name);
}
```

### `cwd`

```ruby
string? cwd()
```

Returns the current working directory, or `none` on error.

### `chdir`

```ruby
bool chdir(path: string)
```

Changes the current working directory. Returns `true` on success.
