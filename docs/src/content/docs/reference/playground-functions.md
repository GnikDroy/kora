---
title: Playground Functions
description: Graphics, input, and timing functions available only in the browser playground, for building small games.
---

The [playground](/kora/play/) adds drawing, input, and timing functions for
building small games in the browser. Draw with them, then switch to the
Canvas tab to see the result.

They exist only in the playground, which injects their declarations
automatically, so you call them directly with no `import` or `extern` needed.
The canvas is 640 by 480; the origin is top-left, with x growing right and y
growing down. Colors are CSS strings like `"#ef4444"` or `"black"`.

## The render loop

Games pace themselves by drawing a frame, sleeping, and repeating. Use
`std/time`'s `sleep` for the delay and `is_key_down` to read the keyboard:

```ruby
import "std/time";

int main() {
    let x = 0;
    while (!is_key_down("q")) {
        draw_clear();
        set_color("#0b1020");
        fill_rect(0, 0, canvas_width(), canvas_height());

        set_color("#22c55e");
        fill_rect(x, 200, 40, 40);
        x = (x + 4) % canvas_width();

        time.sleep(16);   # ~60 frames per second
    }
    return 0;
}
```

## Canvas

```ruby
void  draw_clear()       # clear the whole canvas
int64 canvas_width()
int64 canvas_height()
```

## Color and style

```ruby
void set_color(c: cstring)      # CSS color for fills and strokes, e.g. "#ef4444"
void set_line_width(w: int64)
void set_alpha(a: float64)      # 0.0 transparent .. 1.0 opaque
void set_font_size(px: int64)
```

## Shapes

```ruby
void fill_rect(x: int64, y: int64, w: int64, h: int64)
void stroke_rect(x: int64, y: int64, w: int64, h: int64)
void fill_circle(x: int64, y: int64, r: int64)
void stroke_circle(x: int64, y: int64, r: int64)
void draw_line(x1: int64, y1: int64, x2: int64, y2: int64)
void fill_triangle(x1: int64, y1: int64, x2: int64, y2: int64, x3: int64, y3: int64)
```

## Text

```ruby
void  draw_text(s: cstring, x: int64, y: int64)   # baseline at (x, y)
int64 text_width(s: cstring)                       # width at the current font size
```

## Transforms

Transforms compose, and `save` / `restore` push and pop the current transform so
you can draw a rotated sprite without disturbing the rest of the scene.

```ruby
void save()                              # push the current transform
void restore()                           # pop it
void translate(x: int64, y: int64)
void rotate(a: float64)                  # radians
```

## Keyboard

```ruby
bool is_key_down(key: cstring)   # key names follow the browser, e.g.
                                 # "ArrowLeft", "ArrowRight", " " (space), "q"
```

## Mouse

```ruby
int64 mouse_x()
int64 mouse_y()
bool  is_mouse_down()
```

## A note on timing

`time.sleep` and reading input suspend the program while keeping the browser
responsive, so animation and input work despite JavaScript being single-threaded.
Write a normal loop; the compiler handles it.

For complete examples, open the file menu in the playground and try Snake,
Tetris, Pong, Pacman, Doom, or Mandelbrot.
