---
title: std/algorithm
description: Generic sorting and binary search.
---

Generic sorting and searching. Import with `import "std/algorithm";`.
Available everywhere.

Every function takes a comparator: a struct value with a method
`bool less(self, a: T, b: T)` that returns `true` when `a` orders before `b`.
This is how you supply an ordering for any element type, including your own
structs. Define one once and pass an instance:

```ruby
struct asc {}
impl asc { bool less(self, a: int, b: int) { return a < b; } }

struct desc {}
impl desc { bool less(self, a: int, b: int) { return a > b; } }
```

## Functions

### `less`

```ruby
bool less<T>(a: T, b: T)
```

Returns whether `a` orders before `b`, for any supported type: `int`, `real`,
and `char` by `<`, strings byte-wise lexicographically, and any struct that
defines a `bool __less__(self, other: T)` method.

```ruby
struct asc_of<T> {}
impl asc_of<T> {
    bool less(self, a: T, b: T) { return algorithm.less::<T>(a, b); }
}

algorithm.sort::<string, asc_of<string>>(names, new asc_of<string>);
```

### `sort`

```ruby
void sort<T, C>(xs: [T], cmp: C)
```

Sorts `xs` in place into the order defined by `cmp`. Quicksort: not stable,
O(n log n) on average.

```ruby
import "std/algorithm";

let xs = [3, 1, 2];
algorithm.sort::<int, asc>(xs, new asc);    # xs is now [1, 2, 3]
algorithm.sort::<int, desc>(xs, new desc);  # xs is now [3, 2, 1]
```

### `sort_range`

```ruby
void sort_range<T, C>(xs: [T], lo: int, hi: int, cmp: C)
```

Sorts the elements from index `lo` to index `hi` inclusive, in place,
leaving the rest of the array untouched.

```ruby
let xs = [9, 3, 1, 2, 0];
algorithm.sort_range::<int, asc>(xs, 1, 3, new asc);   # [9, 1, 2, 3, 0]
```

### `binary_search`

```ruby
int? binary_search<T, C>(xs: [T], key: T, cmp: C)
```

Searches `xs` for `key` and returns the index of a match, or `none` if absent.
`xs` must already be sorted with the same comparator. If duplicates exist, any
matching index may be returned.

```ruby
let xs = [1, 2, 3, 5, 8];
algorithm.binary_search::<int, asc>(xs, 5, new asc)   # 3
algorithm.binary_search::<int, asc>(xs, 4, new asc)   # none
```

## Sorting structs

A comparator can order by any field:

```ruby
import "std/algorithm";

struct Player { name: string, score: int }

struct by_score {}
impl by_score {
    bool less(self, a: Player, b: Player) { return a.score > b.score; }
}

int main() {
    let ps = [
        new Player{ name: "ada", score: 30 },
        new Player{ name: "bo",  score: 50 },
    ];
    algorithm.sort::<Player, by_score>(ps, new by_score);
    return 0;   # ps is now bo, ada
}
```
