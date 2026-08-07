---
title: std/iter
description: map, filter, reduce, and friends over arrays.
---

Higher-order helpers over arrays. Import with `import "std/iter";`. Available
everywhere.

Each helper takes a function value. Kora has no closures, so you pass a named
top-level function, and you spell out the type arguments with the turbofish:
`iter.map::<int, string>(xs, f)`. None of them modify the input array.

## Transforming

### `map`

```ruby
[U] map<T, U>(xs: [T], f: U(T))
```

Returns a new array containing `f(x)` for each element `x` of `xs`, in order.

```ruby
int square(x: int) { return x * x; }

iter.map::<int, int>([1, 2, 3], square)   # [1, 4, 9]
```

### `flat_map`

```ruby
[U] flat_map<T, U>(xs: [T], f: [U](T))
```

Applies `f` to each element, where `f` returns an array, and concatenates the
results into one array.

```ruby
[int] twice(x: int) { return [x, x]; }

iter.flat_map::<int, int>([1, 2], twice)   # [1, 1, 2, 2]
```

### `filter`

```ruby
[T] filter<T>(xs: [T], pred: bool(T))
```

Returns the elements of `xs` for which `pred` returns `true`, keeping their
order.

```ruby
bool is_even(x: int) { return x % 2 == 0; }

iter.filter::<int>([1, 2, 3, 4], is_even)   # [2, 4]
```

### `take_while`

```ruby
[T] take_while<T>(xs: [T], pred: bool(T))
```

Returns the longest leading run of elements satisfying `pred`, stopping at the
first element that fails.

### `drop_while`

```ruby
[T] drop_while<T>(xs: [T], pred: bool(T))
```

Returns what remains after that leading run.

```ruby
bool small(x: int) { return x < 3; }

iter.take_while::<int>([1, 2, 5, 1], small)   # [1, 2]
iter.drop_while::<int>([1, 2, 5, 1], small)   # [5, 1]
```

## Reducing

### `reduce`

```ruby
U reduce<T, U>(xs: [T], init: U, f: U(U, T))
```

Folds `xs` from the left: starts with `init`, then replaces the accumulator
with `f(acc, x)` for each element. Returns the final accumulator.

```ruby
int add(acc: int, x: int) { return acc + x; }

iter.reduce::<int, int>([1, 2, 3, 4], 0, add)   # 10
```

### `count`

```ruby
int count<T>(xs: [T], pred: bool(T))
```

Returns how many elements satisfy `pred`.

### `each`

```ruby
void each<T>(xs: [T], f: void(T))
```

Calls `f` on each element in order, for side effects.

```ruby
void show(s: string) { io.print(s); }

iter.each::<string>(["a", "b"], show);
```

## Searching and testing

### `find`

```ruby
T? find<T>(xs: [T], pred: bool(T))
```

Returns the first element satisfying `pred`, or `none` if there is none.

### `position`

```ruby
int? position<T>(xs: [T], pred: bool(T))
```

Returns the index of the first element satisfying `pred`, or `none`.

### `any`

```ruby
bool any<T>(xs: [T], pred: bool(T))
```

Returns `true` if at least one element satisfies `pred`.

### `all`

```ruby
bool all<T>(xs: [T], pred: bool(T))
```

Returns `true` if every element satisfies `pred` (including when `xs` is
empty).

```ruby
bool is_even(x: int) { return x % 2 == 0; }

iter.find::<int>([1, 3, 4], is_even)       # 4
iter.position::<int>([1, 3, 4], is_even)   # 2
iter.any::<int>([1, 3], is_even)           # false
iter.all::<int>([2, 4], is_even)           # true
```

## Chaining

Build pipelines by naming each step:

```ruby
import "std/iter";

bool is_even(x: int)     { return x % 2 == 0; }
int  square(x: int)      { return x * x; }
int  add(a: int, b: int) { return a + b; }

int main() {
    let xs      = [1, 2, 3, 4, 5, 6];
    let evens   = iter.filter::<int>(xs, is_even);       # [2, 4, 6]
    let squares = iter.map::<int, int>(evens, square);   # [4, 16, 36]
    return iter.reduce::<int, int>(squares, 0, add);     # 56
}
```
