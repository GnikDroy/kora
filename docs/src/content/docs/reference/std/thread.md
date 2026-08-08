---
title: std/thread
description: Threads, mutexes, and condition variables.
---

OS threads, mutexes, and condition variables. Import with
`import "std/thread";`. Native backend only.

A thread runs a function of one argument. The argument is the thread's link
back to shared state, so it must be a reference type: a struct or an array.
Guard any state that multiple threads mutate with a `Mutex`.

## Functions

### `spawn`

```ruby
Thread? spawn<T>(entry: void(T), arg: T)
```

Starts a new OS thread running `entry(arg)` and returns its handle, or `none`
if the thread could not be created. `T` must be a struct or array type.

### `mutex`

```ruby
Mutex? mutex()
```

Creates a new, unlocked mutex. Returns `none` on failure.

### `cond`

```ruby
Cond? cond()
```

Creates a new condition variable. Returns `none` on failure.

### `yield`

```ruby
void yield()
```

Hints the scheduler to run another thread. Rarely needed.

## Methods on `Thread`

```ruby
void join(self)     # block until the thread finishes
void detach(self)   # let the thread run on its own; it can no longer be joined
```

Every spawned thread should be either joined or detached.

## Methods on `Mutex`

```ruby
void lock(self)     # block until the lock is acquired
void unlock(self)
```

Hold the lock for the shortest stretch that keeps the shared data consistent,
and always pair each `lock` with an `unlock`.

## Methods on `Cond`

```ruby
void wait(self, m: Mutex)                    # atomically unlock m and sleep; re-locks on wake
bool wait_timeout(self, m: Mutex, ms: int)   # like wait; false if ms elapsed with no wake
void signal(self)                            # wake one waiter
void broadcast(self)                         # wake all waiters
```

Call `wait` with the mutex held, and re-check the condition in a loop after
waking: wakeups can be spurious.

## Example: shared counter

Shared state travels to the thread as its argument. Here four workers bump one
counter:

```ruby
import "std/thread";
import "std/io";
import "std/conv";

struct Counter {
    n: int,
    lock: Mutex,
}

void work(c: Counter) {
    for (let i = 0; i < 1000; i = i + 1) {
        c.lock.lock();
        c.n = c.n + 1;
        c.lock.unlock();
    }
}

int main() {
    let c = new Counter{ n: 0, lock: thread.mutex()! };

    let workers: [Thread] = [];
    for (let i = 0; i < 4; i = i + 1) {
        workers.push(thread.spawn::<Counter>(work, c)!);
    }
    for t | workers {
        t.join();
    }

    io.print(conv.to_string::<int>(c.n));   # 4000
    return 0;
}
```

## Example: waiting on a condition

A consumer sleeps until a producer publishes a value:

```ruby
import "std/thread";

struct Box {
    ready: bool,
    value: int,
    lock: Mutex,
    changed: Cond,
}

void produce(b: Box) {
    b.lock.lock();
    b.value = 42;
    b.ready = true;
    b.changed.signal();
    b.lock.unlock();
}

int main() {
    let b = new Box{ ready: false, value: 0,
                     lock: thread.mutex()!, changed: thread.cond()! };
    let t = thread.spawn::<Box>(produce, b)!;

    b.lock.lock();
    while (!b.ready) {          # re-check after every wake
        b.changed.wait(b.lock);
    }
    let v = b.value;
    b.lock.unlock();

    t.join();
    return v - 42;   # 0
}
```
