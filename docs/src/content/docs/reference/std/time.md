---
title: std/time
description: Sleeping and reading clocks.
---

Sleeping and reading clocks. Import with `import "std/time";`.

`sleep` and `now` work everywhere. `mono_ns` is native only.

## Functions

### `sleep`

```ruby
void sleep(ms: int)
```

Suspends the program for `ms` milliseconds. In the playground this keeps the
browser responsive, which is what makes render loops possible. See
[Playground Functions](../../playground-functions/).

```ruby
time.sleep(16);   # roughly one 60 fps frame
```

### `now`

```ruby
int now()
```

Returns the current wall-clock time as seconds since the Unix epoch.

```ruby
let today = time.now() / 86400;   # days since 1970-01-01
```

### `mono_ns`

```ruby
int mono_ns()
```

Returns a monotonic clock reading in nanoseconds. Native only. The absolute
value is meaningless on its own. Subtract two readings to measure elapsed
time. Unlike `now`, it never jumps backwards.

```ruby
let start = time.mono_ns();
work();
let elapsed_ms = (time.mono_ns() - start) / 1000000;
```
