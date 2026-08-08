---
title: std/conv
description: Converting values to and from strings.
---

Converting values to and from their string forms. Import with
`import "std/conv";`. Available everywhere.

## Functions

### `to_string`

```ruby
string to_string<T>(v: T)
```

Converts any supported value to its string form. The type argument is
required, since Kora does not infer type arguments from the call.

```ruby
conv.to_string::<int>(42)       # "42"
conv.to_string::<bool>(true)    # "true"
conv.to_string::<char>('A')     # "A"
conv.to_string::<Money>(m)      # whatever Money's __str__ returns
```

What each type produces:

| `T` | Result |
| --- | --- |
| `int` | decimal, with a leading `-` if negative |
| `real` | decimal with up to six fractional digits, trailing zeros trimmed (at least one digit always remains). Non-finite values become `"nan"`, `"inf"`, or `"-inf"` |
| `bool` | `"true"` or `"false"` |
| `char` | a one-character string |
| `string` | the value unchanged |
| a struct | whatever its `string __str__(self)` method returns |

```ruby
conv.to_string::<int>(-7)         # "-7"
conv.to_string::<real>(3.14)      # "3.14"
conv.to_string::<real>(2.0)       # "2.0"
conv.to_string::<real>(1.0/3.0)   # "0.333333"
```

Any other `T` is an error at the call site, reported as a missing `__str__`
method on the type.

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
