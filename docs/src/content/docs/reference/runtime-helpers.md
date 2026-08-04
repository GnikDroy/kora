---
title: Runtime Helpers
description: The Kora standard library, available on every backend, native and JavaScript.
---

The standard library is a set of modules written in Kora, available on every
runtime. Import by path; a module is named after its last path segment, so
`import "std/math";` gives `math.abs`, `math.gcd`, and so on (rename with
`import "std/math" m;`). For browser-only graphics and input, see
[Playground Functions](../playground-functions/).

## std/io

Standard input and output.

```ruby
void   write(s: string)   # write a string to stdout, no trailing newline
void   print(s: string)   # write a string followed by a newline
string? input()           # read one line from stdin; none at end of input
```

## std/conv

Convert between values and their string forms.

```ruby
string int_to_string(n: int)
string real_to_string(x: real)
string bool_to_string(b: bool)
string char_to_string(c: char)
int?   string_to_int(s: string)   # none if the string is not a valid integer
```

## std/str

String helpers. Recall that a `string` is an array of `char` (bytes).

```ruby
bool     is_space(c: char)
int?     index_of(haystack: string, needle: string)   # none if not found
bool     contains(haystack: string, needle: string)
bool     starts_with(s: string, prefix: string)
bool     ends_with(s: string, suffix: string)
string   to_upper(s: string)
string   to_lower(s: string)
string   trim(s: string)                # strip leading/trailing whitespace
string   repeat(s: string, n: int)
string   reverse(s: string)
[string] split(s: string, sep: char)
string   join(parts: [string], sep: string)
```

## std/math

Integer and floating-point math. Integer helpers take and return `int`;
floating-point helpers take and return `real`. The trigonometric and exponential
functions are backed by the platform math library.

```ruby
real random()                          # in [0, 1)

# integer
int  abs(n: int)
int  min(a: int, b: int)
int  max(a: int, b: int)
int  clamp(v: int, lo: int, hi: int)
int  pow(base: int, exp: int)
int  gcd(a: int, b: int)
int  sign(n: int)                      # -1, 0, or 1

# real
real absf(x: real)
real signf(x: real)
real minf(a: real, b: real)
real maxf(a: real, b: real)
real clampf(v: real, lo: real, hi: real)
real floorf(x: real)
real ceilf(x: real)
real roundf(x: real)
real truncf(x: real)
real sqrtf(x: real)
real powf(base: real, exponent: real)
real fmod(x: real, y: real)
real hypot(x: real, y: real)
real cbrt(x: real)

# exponential and logarithmic
real exp(x: real)
real log(x: real)
real log2(x: real)
real log10(x: real)

# trigonometric
real sin(x: real)    real cos(x: real)    real tan(x: real)
real asin(x: real)   real acos(x: real)   real atan(x: real)
real atan2(y: real, x: real)
real sinh(x: real)   real cosh(x: real)   real tanh(x: real)
```

## std/fs

File I/O. `open` returns an optional; check it before use, and `close` when done.
Available on the native backend.

```ruby
File? open(path: string, mode: string)   # mode is a C fopen mode, e.g. "r", "w"
bool  remove(path: string)
bool  rename(from: string, to: string)
```

Methods on `File`:

```ruby
char?   read_char(self)     # none at end of file
string? read_line(self)     # one line without the newline; none at end
string  read_all(self)      # the rest of the file
void    write(self, s: string)
void    flush(self)
void    close(self)
int     tell(self)          # current offset
void    seek(self, offset: int)
```

## std/env

Read environment variables.

```ruby
string? get(name: string)   # none if the variable is not set
```

## std/proc

Run commands and exit the process.

```ruby
int  run(cmd: string)   # run a shell command, returns its exit status
void exit(code: int)
```

## std/time

```ruby
void sleep(ms: int)   # sleep for the given milliseconds
int  now()            # seconds since the Unix epoch
```

## std/algorithm

Generic sorting and searching. Both take a **comparator**: a struct value with a
method `bool less(self, a: T, b: T)`. This is how ordering is supplied without
built-in operators over generic types.

```ruby
void sort<T, C>(xs: [T], cmp: C)                        # in place
void sort_range<T, C>(xs: [T], lo: int, hi: int, cmp: C)
int? binary_search<T, C>(xs: [T], key: T, cmp: C)       # index, or none
```

Define a comparator once and pass an instance:

```ruby
import "std/algorithm";

struct asc {}
impl asc { bool less(self, a: int, b: int) { return a < b; } }

int main() {
    let xs = [3, 1, 2];
    sort::<int, asc>(xs, new asc);           # [1, 2, 3]
    let i = binary_search::<int, asc>(xs, 2, new asc);
    return 0;
}
```

## std/collections

Generic containers. Each module exposes a `make` constructor and a type named
after the module. Construct with turbofish: `stack.make::<int>()`.

### stack

A last-in, first-out stack. `stack<T>`.

```ruby
stack<T> make<T>()

void push(self, x: T)
T    pop(self)       # panics if empty
T    peek(self)      # panics if empty
int  count(self)
bool empty(self)
```

### queue

A first-in, first-out queue. `queue<T>`.

```ruby
queue<T> make<T>()

void enqueue(self, x: T)
T    dequeue(self)   # panics if empty
T    peek(self)      # panics if empty
int  count(self)
bool empty(self)
```

### list

A doubly linked list. `list<T>`.

```ruby
list<T> make<T>()

void push_front(self, x: T)
void push_back(self, x: T)
T    pop_front(self)     # panics if empty
T?   front(self)         # none if empty
T    get(self, index: int)
int  count(self)
bool empty(self)
```

### set and map

`set<K, H>` and `map<K, V, H>` are open-addressed hash containers. Because Kora
has no built-in `hash()`, the hash strategy is a **type parameter** `H`: a struct
with a method `int hash(self, key: K)`. Ready-made hashers live in
`std/collections/hasher` (see below).

```ruby
# set<K, H>
set<K, H> make<K, H>()

bool add(self, key: K)      # true if newly added
bool has(self, key: K)
bool remove(self, key: K)   # true if it was present
int  count(self)
```

```ruby
# map<K, V, H>
map<K, V, H> make<K, V, H>()

void set(self, key: K, value: V)
V?   get(self, key: K)      # none if absent
bool has(self, key: K)
bool remove(self, key: K)   # true if it was present
int  count(self)
```

Putting it together:

```ruby
import "std/collections/map";
import "std/collections/hasher";

int main() {
    let counts = map.make::<string, int, string_hasher>();
    counts.set("apples", 3);
    let n = counts.get("apples");   # some(3)
    return counts.count();          # 1
}
```

### hasher

Ready-made hash strategies for the common key types. Each is an empty struct
with a method `int hash(self, key: ...)`, used as the `H` parameter of `set` and
`map`.

```ruby
struct int_hasher    {}   # keys of type int
struct string_hasher {}   # keys of type string
struct char_hasher   {}   # keys of type char
struct bool_hasher   {}   # keys of type bool
```

To hash a key type of your own, write a struct with a matching
`int hash(self, key: YourType)` method and pass it as `H`.
