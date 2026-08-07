---
title: std/collections
description: "Generic containers: stack, queue, list, set, and map."
---

Generic containers. Each container is its own module under
`std/collections/`, exposing a `make` constructor and a type named after the
module. Available everywhere.

```ruby
import "std/collections/stack";

let s = stack.make::<int>();
```

Keys and elements can be any type. `set` and `map` additionally need a hash
strategy (see [hasher](#hasher) below) and keys that support `==` (`int`,
`string`, `char`, `bool`).

## stack

`import "std/collections/stack";` gives `stack<T>`, a last-in, first-out
stack.

```ruby
stack<T> make<T>()        # a new empty stack

void push(self, x: T)     # put x on top
T    pop(self)            # remove and return the top; panics if empty
T    peek(self)           # return the top without removing; panics if empty
int  count(self)          # number of elements
bool empty(self)          # true if count() == 0
```

```ruby
import "std/collections/stack";

let s = stack.make::<int>();
s.push(1);
s.push(2);
s.peek();    # 2
s.pop();     # 2
s.count();   # 1
```

## queue

`import "std/collections/queue";` gives `queue<T>`, a first-in, first-out
queue with amortized O(1) operations.

```ruby
queue<T> make<T>()        # a new empty queue

void enqueue(self, x: T)  # add x at the back
T    dequeue(self)        # remove and return the front; panics if empty
T    peek(self)           # return the front without removing; panics if empty
int  count(self)          # number of elements
bool empty(self)          # true if count() == 0
```

```ruby
import "std/collections/queue";

let q = queue.make::<string>();
q.enqueue("first");
q.enqueue("second");
q.dequeue();   # "first"
```

## list

`import "std/collections/list";` gives `list<T>`, a singly linked list with
O(1) insertion at both ends.

```ruby
list<T> make<T>()            # a new empty list

void push_front(self, x: T)  # insert at the front
void push_back(self, x: T)   # append at the back
T    pop_front(self)         # remove and return the front; panics if empty
T?   front(self)             # the front element, or none if empty
T    get(self, index: int)   # element at index; O(index), panics if out of range
int  count(self)             # number of elements
bool empty(self)             # true if count() == 0
```

```ruby
import "std/collections/list";

let l = list.make::<int>();
l.push_back(2);
l.push_front(1);
l.get(1);        # 2
l.pop_front();   # 1
```

## set

`import "std/collections/set";` gives `set<K, H>`, an open-addressed hash set.
`K` is the key type and `H` is a hasher struct with a method
`int hash(self, key: K)`.

```ruby
set<K, H> make<K, H>()      # a new empty set

bool add(self, key: K)      # insert; true if the key was newly added
bool has(self, key: K)      # true if the key is present
bool remove(self, key: K)   # delete; true if the key was present
int  count(self)            # number of keys
```

```ruby
import "std/collections/set";
import "std/collections/hasher";

let seen = set.make::<int, int_hasher>();
seen.add(3);    # true
seen.add(3);    # false, already present
seen.has(3);    # true
seen.count();   # 1
```

## map

`import "std/collections/map";` gives `map<K, V, H>`, an open-addressed hash
map from `K` to `V`. `H` is a hasher for `K`, as with `set`.

```ruby
map<K, V, H> make<K, V, H>()      # a new empty map

void set(self, key: K, value: V)  # insert, or overwrite an existing key
V?   get(self, key: K)            # the value for key, or none if absent
bool has(self, key: K)            # true if the key is present
bool remove(self, key: K)         # delete; true if the key was present
int  count(self)                  # number of entries
```

```ruby
import "std/collections/map";
import "std/collections/hasher";

let counts = map.make::<string, int, string_hasher>();
counts.set("apples", 3);
counts.set("apples", 4);      # overwrite
counts.get("apples");         # 4 (as int?)
counts.get("pears");          # none
counts.count();               # 1
```

## hasher

`import "std/collections/hasher";` provides ready-made hash strategies for the
common key types. Each is an empty struct with a method
`int hash(self, key: ...)`, used as the `H` type parameter of `set` and `map`.

```ruby
struct int_hasher    {}   # for int keys
struct string_hasher {}   # for string keys
struct char_hasher   {}   # for char keys
struct bool_hasher   {}   # for bool keys
```

To use keys of your own type, write a struct with a matching
`int hash(self, key: YourType)` method and pass it as `H`. Equal keys must
hash equally:

```ruby
import "std/collections/set";

struct Point { x: int, y: int }

struct point_hasher {}
impl point_hasher {
    int hash(self, key: Point) { return key.x * 31 + key.y; }
}

let visited = set.make::<Point, point_hasher>();
```
