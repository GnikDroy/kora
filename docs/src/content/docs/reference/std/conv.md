---
title: std/conv
description: Converting values to and from strings.
---

Converting values to and from their string forms. Import with
`import "std/conv";`. Available everywhere.

## Functions

### `int_to_string`

```ruby
string int_to_string(n: int)
```

Returns the decimal representation of `n`, with a leading `-` if negative.

```ruby
conv.int_to_string(42)     # "42"
conv.int_to_string(-7)     # "-7"
```

### `real_to_string`

```ruby
string real_to_string(x: real)
```

Returns `x` in decimal with up to six fractional digits, trailing zeros
trimmed (at least one digit always remains). Non-finite values become `"nan"`,
`"inf"`, or `"-inf"`.

```ruby
conv.real_to_string(3.14)     # "3.14"
conv.real_to_string(2.0)      # "2.0"
conv.real_to_string(1.0/3.0)  # "0.333333"
```

### `bool_to_string`

```ruby
string bool_to_string(b: bool)
```

Returns `"true"` or `"false"`.

### `char_to_string`

```ruby
string char_to_string(c: char)
```

Returns a one-character string containing `c`.

```ruby
conv.char_to_string('A')   # "A"
```

### `string_to_int`

```ruby
int? string_to_int(s: string)
```

Parses `s` as a decimal integer: an optional leading `+` or `-`, then digits.
Returns `none` if `s` is empty or contains anything else, including
whitespace.

```ruby
conv.string_to_int("42")     # 42
conv.string_to_int("-13")    # -13
conv.string_to_int("4x")     # none
conv.string_to_int(" 42")    # none (no whitespace allowed)
```

A typical use, defaulting on bad input:

```ruby
let parsed = conv.string_to_int(text);
let port = 80;
if (parsed != none) { port = parsed!; }
```
