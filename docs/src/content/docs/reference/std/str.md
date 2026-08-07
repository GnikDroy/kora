---
title: std/str
description: String searching, splitting, and transforming.
---

String helpers. Import with `import "std/str";`. Available everywhere.

A `string` is an array of `char` (bytes), so these functions work byte-wise.
Case conversion covers ASCII letters only. For length, indexing, and slicing
use the built-in array methods (`s.len()`, `s[i]`, `s.slice(a, b)`).

## Searching

### `index_of`

```ruby
int? index_of(haystack: string, needle: string)
```

Returns the index of the first occurrence of `needle` in `haystack`, or `none`
if it does not occur. An empty `needle` matches at index 0.

```ruby
str.index_of("hello world", "world")   # 6
str.index_of("hello", "xyz")           # none
```

### `contains`

```ruby
bool contains(haystack: string, needle: string)
```

Returns `true` if `needle` occurs anywhere in `haystack`.

```ruby
str.contains("hello world", "lo w")   # true
```

### `starts_with`

```ruby
bool starts_with(s: string, prefix: string)
```

Returns `true` if `s` begins with `prefix`.

### `ends_with`

```ruby
bool ends_with(s: string, suffix: string)
```

Returns `true` if `s` ends with `suffix`.

```ruby
str.starts_with("main.kora", "main")   # true
str.ends_with("main.kora", ".kora")    # true
```

## Transforming

### `to_upper`

```ruby
string to_upper(s: string)
```

Returns a copy of `s` with ASCII letters `a` to `z` uppercased.

### `to_lower`

```ruby
string to_lower(s: string)
```

Returns a copy of `s` with ASCII letters `A` to `Z` lowercased.

```ruby
str.to_upper("kora 1.0")   # "KORA 1.0"
str.to_lower("KoRa")       # "kora"
```

### `trim`

```ruby
string trim(s: string)
```

Returns `s` without leading and trailing whitespace (spaces, tabs, newlines,
carriage returns).

```ruby
str.trim("  hi \n")   # "hi"
```

### `repeat`

```ruby
string repeat(s: string, n: int)
```

Returns `s` concatenated `n` times. Returns `""` when `n` is zero or negative.

```ruby
str.repeat("ab", 3)   # "ababab"
```

### `reverse`

```ruby
string reverse(s: string)
```

Returns `s` with its bytes in reverse order.

```ruby
str.reverse("kora")   # "arok"
```

## Splitting and joining

### `split`

```ruby
[string] split(s: string, sep: char)
```

Splits `s` at every occurrence of `sep`. The separator is not included in the
pieces. `n` separators always produce `n + 1` pieces, so adjacent separators
yield empty strings.

```ruby
str.split("a,b,c", ',')   # ["a", "b", "c"]
str.split("a,,c", ',')    # ["a", "", "c"]
str.split("abc", ',')     # ["abc"]
```

### `join`

```ruby
string join(parts: [string], sep: string)
```

Concatenates `parts` with `sep` between consecutive elements.

```ruby
str.join(["usr", "local", "bin"], "/")   # "usr/local/bin"
```

## Classifying

### `is_space`

```ruby
bool is_space(c: char)
```

Returns `true` if `c` is a space, tab, newline, or carriage return.

```ruby
str.is_space(' ')   # true
str.is_space('x')   # false
```
