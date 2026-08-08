---
title: std/math
description: Integer and floating-point math, and random numbers.
---

Math over `int` and `real`, plus a random number generator. Import with
`import "std/math";`. Available everywhere.

Integer helpers take and return `int`. Floating-point helpers take and return
`real`, and most carry an `f` suffix (`abs` vs `absf`). The exponential and
trigonometric functions are backed by the platform math library.

## Constants

```ruby
let PI = 3.141592653589793;
let TAU = PI * 2.0;
let E = 2.718281828459045;
```

```ruby
let degrees = radians * 180.0 / math.PI;
```

## Random numbers

```ruby
real random()      # uniform in [0, 1)
void seed(s: int)  # seed the generator; same seed, same sequence
```

`random` returns a uniform `real` in `[0, 1)`. It is deterministic per seed and
not suitable for cryptography.

```ruby
math.seed(7);
let dice = (math.random() * 6.0) as int + 1;   # 1 to 6
```

## Integer functions

```ruby
int abs(n: int)                      # absolute value
int sign(n: int)                     # -1, 0, or 1
int min(a: int, b: int)              # smaller of the two
int max(a: int, b: int)              # larger of the two
int clamp(v: int, lo: int, hi: int)  # v limited to [lo, hi]
int pow(base: int, exp: int)         # base to the power exp; 0 if exp < 0
int gcd(a: int, b: int)              # greatest common divisor, never negative
```

```ruby
math.abs(-5)          # 5
math.clamp(12, 0, 9)  # 9
math.pow(2, 10)       # 1024
math.gcd(48, 36)      # 12
```

## Real functions

```ruby
real absf(x: real)                          # absolute value
real signf(x: real)                         # -1.0, 0.0, or 1.0
real minf(a: real, b: real)                 # smaller of the two
real maxf(a: real, b: real)                 # larger of the two
real clampf(v: real, lo: real, hi: real)    # v limited to [lo, hi]
real floorf(x: real)                        # round down
real ceilf(x: real)                         # round up
real roundf(x: real)                        # round half away from zero
real truncf(x: real)                        # round toward zero
real sqrtf(x: real)                         # square root
real cbrt(x: real)                          # cube root
real powf(base: real, exponent: real)       # base to the power exponent
real fmod(x: real, y: real)                 # remainder of x / y, sign of x
real hypot(x: real, y: real)                # sqrt(x*x + y*y), without overflow
```

```ruby
math.floorf(2.7)        # 2.0
math.roundf(-2.5)       # -3.0
math.fmod(7.5, 2.0)     # 1.5
math.hypot(3.0, 4.0)    # 5.0
```

## Exponentials and logarithms

```ruby
real exp(x: real)     # e to the x
real log(x: real)     # natural log
real log2(x: real)    # base-2 log
real log10(x: real)   # base-10 log
```

```ruby
math.log2(1024.0)   # 10.0
```

## Trigonometry

All angles are in radians.

```ruby
real sin(x: real)    real cos(x: real)    real tan(x: real)
real asin(x: real)   real acos(x: real)   real atan(x: real)
real atan2(y: real, x: real)   # angle of the point (x, y); handles quadrants
real sinh(x: real)   real cosh(x: real)   real tanh(x: real)
```

```ruby
let pi = math.acos(-1.0);
math.sin(pi / 2.0)          # 1.0
math.atan2(1.0, 1.0)        # 0.785398... (pi / 4)
```
