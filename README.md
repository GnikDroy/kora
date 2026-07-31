<h1>
    <img src="logo.svg" alt="Kora" width="64" height="64" align="center" />
    <span> Kora </span>
</h1>

Kora is a small, statically typed, programming language with a garbage collector. It compiles to native executables and JavaScript. Your programs run right
in the browser with nothing to install.

**[Try Kora in the online playground](https://gnikdroy.github.io/kora/)**

## Features

- Familiar C-like syntax: `if` / `while` / `for`, functions, and blocks you
  already know.
- Static typing with inference: `int`, `real`, `char`, `bool`,
  arrays, and strings.
- Arrays and strings with handy built-in methods (`push`, `pop`, `insert`,
  `remove`, `slice`, `extend`, `len`). A `string` is just an array of `char`.
- Structs and methods: POD types `struct`, attach behavior with
  `impl`.
- Optionals (`T?`) instead of null. [The billion-dollar mistake](https://computerhistory.org/blog/in-memoriam-sir-antony-hoare-1934-2026/)
- Runtime safety is a design choice: array indexing is
  bounds-checked, integer division by zero, `pop()` on an empty array, and
  force-unwrapping `none` all panic with a clear message instead of
  corrupting memory.
- Modules which allow you to split a program, plus a small
  standard library (I/O, string helpers, math, and conversions) written in
  Kora.
- The native runtime written in C, the lingua-franca of programming languages.
- Runs in the browser through a WebAssembly-powered playground, and compiles
  to native binaries through an LLVM backend (LLVM 22 is required).

## A taste of Kora

Sudoku solver in kora using arrays, recursion, and backtracking:

```ruby
# -------------------------------------------------------
# Please bear with the syntax highlighting ;(
# I selected ruby as the language for the code blocks.
# The playground has proper syntax highlighting for kora.
# -------------------------------------------------------

import "std/io";

# Sudoku solver via backtracking. The board is a flat 81-cell int
# array indexed as row * 9 + col; 0 marks an empty cell.

void print_board(board: [int]) {
    for (let row = 0; row < 9; row = row + 1) {
        if (row == 3 || row == 6) {
            io.write("------+-------+------\n");
        }
        for (let col = 0; col < 9; col = col + 1) {
            if (col == 3 || col == 6) { io.write("| "); }
            let v = board[row * 9 + col];
            if (v == 0) {
                io.write(". ");
            } else {
                io.write([(v + 48) as char, ' ']);
            }
        }
        io.write("\n");
    }
}

# Would `val` at `idx` conflict with its row, column, or 3x3 box?
bool is_valid(board: [int], idx: int, val: int) {
    let row = idx / 9;
    let col = idx % 9;
    for (let i = 0; i < 9; i = i + 1) {
        if (board[row * 9 + i] == val) { return false; }
        if (board[i * 9 + col] == val) { return false; }
        let r = (row / 3) * 3 + i / 3;
        let c = (col / 3) * 3 + i % 3;
        if (board[r * 9 + c] == val) { return false; }
    }
    return true;
}

# Find the first empty cell, try 1..9, recurse on any digit that keeps
# the board valid, unwind if none fit.
bool solve(board: [int]) {
    for (let idx = 0; idx < 81; idx = idx + 1) {
        if (board[idx] == 0) {
            for (let val = 1; val <= 9; val = val + 1) {
                if (is_valid(board, idx, val)) {
                    board[idx] = val;
                    if (solve(board)) { return true; }
                    board[idx] = 0;
                }
            }
            return false;
        }
    }
    return true;
}

# '1'..'9' are givens; anything else marks an empty cell.
void load_puzzle(board: [int], puzzle: string) {
    for (let i = 0; i < 81; i = i + 1) {
        let d = (puzzle[i] as int) - 48;
        if (d >= 1 && d <= 9) {
            board[i] = d;
        } else {
            board[i] = 0;
        }
    }
}

int main() {
    let board = new int[81];

    # Classic hard-ish puzzle from the Wikipedia Sudoku page.
    load_puzzle(board, "530070000600195000098000060800060003400803001700020006060000280000419005000080079");

    io.write("Puzzle:\n\n");
    print_board(board);

    if (solve(board)) {
        io.write("\nSolved:\n\n");
        print_board(board);
    } else {
        io.write("\nNo solution found.\n");
    }
    return 0;
}
```

For bigger programs, the [`res/`](res/) folder has this Sudoku solver plus
playable versions of Snake, Tetris, Pong, Pacman, Doom, and a Mandelbrot
renderer.

## Try it locally

```bash
wasm-pack build --target web --out-name compiler -- --no-default-features # build the wasm compiler
python -m http.server # serve at localhost
```

Then open the printed URL and start typing.

To compile programs to native executables instead, build the compiler.
Building the compiler requires lib LLVM 22 and a C compiler for the platform linker.

```bash
cargo build --release # build the compiler
./target/release/kora program.kora -o program # native standalone binary
./target/release/kora program.kora --emit-js  # or print the JavaScript
```

Running the test suite is `cargo test` (also requires LLVM 22).

## Grammar

```ebnf
module      = { import | struct | impl | extern | function } ;

import      = "import" STRING [ ident ] ";" ;

struct      = "struct" ident "{" [ member { "," member } [ "," ] ] "}" ;
member      = ident ":" type ;

impl        = "impl" ident "{" { method } "}" ;
method      = rettype ident "(" "self" [ "," [ param { "," param } [ "," ] ] ] ")" block ;

extern      = "extern" ( "void" | externtype ) ident "(" externparams ")" ";" ;
externparams= [ externparam { "," externparam } [ "," ] ] ;
externparam = ident ":" externtype ;
externtype  = "int8" | "int16" | "int32" | "int64"       (* C types only *)
            | "uint8" | "uint16" | "uint32" | "uint64"
            | "float32" | "float64" | "bool" | "char"
            | "cint" | "cuint" | "clong" | "culong" | "csize"
            | ( "cstring" | "opaque" ) [ "?" ] ;

function    = rettype ident "(" params ")" block ;
rettype     = "void" | type ;
params      = [ param { "," param } [ "," ] ] ;
param       = ident ":" type ;

type        = basetype [ "?" ] ;                (* "?" makes it optional *)
basetype    = "int" | "real" | "char" | "bool" | "string" | "opaque"
            | ident                      (* struct name *)
            | "[" type "]" ;

statement   = ";"
            | expr ";"
            | "let" ident [ ":" type ] "=" expr ";"
            | "return" [ expr ] ";"
            | "break" ";"
            | "continue" ";"
            | "if" "(" expr ")" statement [ "else" statement ]
            | "while" "(" expr ")" statement
            | "for" "(" forinit expr ";" expr ")" statement
            | "for" ident "|" expr statement
            | block ;
forinit     = ";" | expr ";" | "let" ident [ ":" type ] "=" expr ";" ;
block       = "{" { statement } "}" ;

expr        = assign ;
assign      = or [ "=" assign ] ;
or          = and  { "||" and } ;
and         = eq   { "&&" eq } ;
eq          = rel  { ( "==" | "!=" ) rel } ;
rel         = add  { ( "<" | ">" | "<=" | ">=" ) add } ;
add         = mul  { ( "+" | "-" | "|" | "^" ) mul } ;
mul         = cast { ( "*" | "/" | "%" | "&" | "<<" | ">>" ) cast } ;
cast        = unary { "as" type } ;
unary       = ( "!" | "-" ) unary | postfix ;
postfix     = primary { "(" args ")" | "[" expr "]" | "." ident | "!" } ;
args        = [ expr { "," expr } [ "," ] ] ;
primary     = INT | REAL | CHAR | STRING | "true" | "false" | "none"
            | ident
            | "(" expr ")"
            | "[" [ expr { "," expr } [ "," ] ] "]"
            | "new" type [ "[" expr "]"
                         | "{" [ field { "," field } [ "," ] ] "}" ] ;
field       = ident ":" expr ;
```
