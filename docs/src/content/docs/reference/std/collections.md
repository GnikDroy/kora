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

Keys and elements can be any type. `set` and `map` additionally need keys that
support `==`, which today means `int`, `string`, `char`, or `bool`. They hash
those keys through [hasher](#hasher) with no setup on your part.

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

`import "std/collections/set";` gives `set<K>`, an open-addressed hash set
keyed by `K`.

```ruby
set<K> make<K>()            # a new empty set

bool add(self, key: K)      # insert; true if the key was newly added
bool has(self, key: K)      # true if the key is present
bool remove(self, key: K)   # delete; true if the key was present
int  count(self)            # number of keys
[K]  items(self)            # the keys, in no particular order
```

```ruby
import "std/collections/set";

let seen = set.make::<int>();
seen.add(3);    # true
seen.add(3);    # false, already present
seen.has(3);    # true
seen.count();   # 1

for x | seen.items() {
    # visit each element
}
```

## map

`import "std/collections/map";` gives `map<K, V>`, an open-addressed hash map
from `K` to `V`, keyed the same way as `set`.

```ruby
map<K, V> make<K, V>()            # a new empty map

void set(self, key: K, value: V)  # insert, or overwrite an existing key
V?   get(self, key: K)            # the value for key, or none if absent
bool has(self, key: K)            # true if the key is present
bool remove(self, key: K)         # delete; true if the key was present
int  count(self)                  # number of entries
[K]  keys(self)                   # the keys, in no particular order
[V]  values(self)                 # the values, in the same order as keys()
```

`keys()` and `values()` walk the map the same way, so as long as the map is
not modified in between, `keys()[i]` maps to `values()[i]`:

```ruby
import "std/collections/map";
import "std/conv";
import "std/io";

int main() {
    let counts = map.make::<string, int>();
    counts.set("apples", 3);
    counts.set("pears", 5);

    let ks = counts.keys();
    let vs = counts.values();
    for (let i = 0; i < ks.len(); i = i + 1) {
        io.print(ks[i] + ": " + conv.to_string::<int>(vs[i]));
    }
    return 0;
}
```

## hasher

`import "std/collections/hasher";` gives the one hash function that `set` and
`map` use for their keys.

```ruby
int hash<T>(key: T)
```

It hashes `int`, `string`, `char`, and `bool` by value, and defers to an
`int __hash__(self)` method on any other struct. `set` and `map` call it for
you, so you only import this module to hash something yourself:

```ruby
import "std/collections/hasher";

struct Point { x: int, y: int }
impl Point {
    int __hash__(self) { return self.x * 31 + self.y; }
}

let h = hasher.hash::<Point>(new Point{ x: 1, y: 2 });
```

