---
title: std/proc
description: Running commands and controlling the process.
---

Running commands and controlling the current process. Import with
`import "std/proc";`.

`run` and `exit` work on the native backend and Node. `capture`, `pid`, and
`abort` are native only.

## Functions

### `run`

```ruby
int run(cmd: string)
```

Runs `cmd` through the system shell, with output going to the program's own
stdout and stderr. Blocks until the command finishes and returns its exit
status.

```ruby
let status = proc.run("ls -l");
if (status != 0) {
    io.print("command failed");
}
```

### `capture`

```ruby
string capture(cmd: string)
```

Runs `cmd` through the system shell and returns its captured standard output.
Returns `""` if the command could not be started, so an empty result is
indistinguishable from failure.

```ruby
let branch = str.trim(proc.capture("git branch --show-current"));
```

### `pid`

```ruby
int pid()
```

Returns this process's id.

### `exit`

```ruby
void exit(code: int)
```

Terminates the process immediately with the given exit code. Never returns.

```ruby
if (fatal) {
    proc.exit(1);
}
```

### `abort`

```ruby
void abort()
```

Terminates abnormally (raises `SIGABRT`). Use `exit` for normal termination.
`abort` signals a bug and may produce a core dump.
