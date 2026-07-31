#![cfg(feature = "codegen")]

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kora-e2e-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn frontend(dir: &Path, files: &[(&str, &str)]) -> kora::CompiledProgram {
    for (name, source) in files {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let entry = dir.join(files[0].0);
    kora::compile(entry.to_str().unwrap(), |path: &Path| {
        std::fs::read_to_string(path).ok()
    })
    .expect("front-end")
}

fn exec(cmd: &mut Command, stdin: &[u8]) -> (String, String, i32) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn run_program_with_stdin(files: &[(&str, &str)], stdin: &[u8]) -> (String, String, i32) {
    let dir = temp_dir();
    let program = frontend(&dir, files);

    let binary = dir.join("main");
    kora::backend::native(&program, &binary).expect("build");
    let native = exec(&mut Command::new(&binary), stdin);

    let js = kora::backend::node_program(program, HashSet::new()).expect("emit js");
    let script = dir.join("main.js");
    std::fs::write(&script, js).unwrap();
    let node = exec(Command::new("node").arg(&script), stdin);

    assert_eq!(native.0, node.0, "stdout diverged; node stderr: {}", node.1);
    assert_eq!(
        native.2, node.2,
        "exit code diverged; node stderr: {}",
        node.1
    );
    std::fs::remove_dir_all(&dir).ok();
    native
}

fn run_program(files: &[(&str, &str)]) -> (String, String, i32) {
    run_program_with_stdin(files, b"")
}

fn run(source: &str) -> (String, String, i32) {
    run_program(&[("main.kora", source)])
}

fn run_native_only(source: &str) -> (String, String, i32) {
    let dir = temp_dir();
    let program = frontend(&dir, &[("main.kora", source)]);
    let binary = dir.join("main");
    kora::backend::native(&program, &binary).expect("build");
    let out = exec(&mut Command::new(&binary), b"");
    std::fs::remove_dir_all(&dir).ok();
    out
}

#[test]
fn test_native_libc_bindings() {
    let (_, stderr, code) = run_native_only(
        r#"
            extern cint abs(x: cint);
            extern cint atoi(s: cstring);
            extern csize strlen(s: cstring);
            extern cstring? getenv(name: cstring);
            int main() {
                let r = 0;
                if (abs(0 - 42) == 42) { r = r + 1; }
                if (atoi("123") == 123) { r = r + 2; }
                if (strlen("hello") == 5) { r = r + 4; }
                if (getenv("KORA_E2E_DEFINITELY_UNSET") == none) { r = r + 8; }
                let path = getenv("PATH");
                if (path != none && path!.len() > 0) { r = r + 16; }
                return r;
            }
        "#,
    );
    assert_eq!(code, 31, "{stderr}");
}

#[test]
fn test_fib() {
    let (stdout, stderr, code) = run(r#"
            import "std/conv";
            import "std/io";
            int fib(n: int) {
                if (n < 2) { return n; }
                return fib(n - 1) + fib(n - 2);
            }
            int main() {
                for (let i = 0; i < 10; i = i + 1) { io.print(conv.int_to_string(fib(i))); }
                return 0;
            }
        "#);
    assert_eq!(stdout, "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n", "{stderr}");
    assert_eq!(code, 0);
}

#[test]
fn test_exit_code() {
    let (_, _, code) = run("int main() { return 42; }");
    assert_eq!(code, 42);
}

#[test]
fn test_division_by_zero_panics() {
    let (_, stderr, code) = run("int main() { let z = 0; return 1 / z; }");
    assert!(stderr.contains("division by zero"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_modulo_by_zero_panics() {
    let (_, stderr, code) = run("int main() { let z = 0; return 1 % z; }");
    assert!(stderr.contains("division by zero"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_struct_literals_and_aliasing() {
    let (_, _, code) = run(r#"
            struct Point { x: int, y: int }
            int main() {
                let p = new Point { x: 1, y: 2 };
                let q = p;
                q.x = 10;
                return p.x * 10 + p.y;
            }
        "#);
    assert_eq!(code, 102);
}

#[test]
fn test_struct_mixed_member_layout() {
    let (_, _, code) = run(r#"
            struct Mixed { c: char, r: real, b: bool, n: int }
            int main() {
                let m = new Mixed { c: 'a', r: 2.5, b: true, n: 7 };
                if (m.b && m.c == 'a' && m.r == 2.5) { return m.n; }
                return 0;
            }
        "#);
    assert_eq!(code, 7);
}

#[test]
fn test_struct_defaults_are_zeroed_and_distinct() {
    let (_, _, code) = run(r#"
            struct Inner { a: int, b: real }
            struct Outer { i: Inner, n: int }
            int main() {
                let o = new Outer;
                let p = new Outer;
                o.i.a = 7;
                return o.i.a * 10 + p.i.a + o.n;
            }
        "#);
    assert_eq!(code, 70);
}

#[test]
fn test_methods() {
    let (_, _, code) = run(r#"
            struct Counter { n: int }
            impl Counter {
                void bump(self) { self.n = self.n + 1; }
                int plus(self, extra: int) { return self.n + extra; }
                Counter me(self) { return self; }
            }
            int main() {
                let c = new Counter;
                c.bump();
                c.me().bump();
                return c.plus(40);
            }
        "#);
    assert_eq!(code, 42);
}

#[test]
fn test_structs_as_function_values() {
    let (_, _, code) = run(r#"
            struct Vec2 { x: int, y: int }
            int dot(a: Vec2, b: Vec2) { return a.x * b.x + a.y * b.y; }
            Vec2 make(x: int, y: int) { return new Vec2 { x: x, y: y }; }
            int main() {
                return dot(make(1, 2), make(3, 4));
            }
        "#);
    assert_eq!(code, 11);
}

#[test]
fn test_array_literals_index_len() {
    let (_, _, code) = run(r#"
            int main() {
                let a = [10, 20, 30];
                a[1] = a[0] + 5;
                let x = 0;
                x = a[2] = 99;
                return a.len() * 10 + a[1] + x - 99 - 15 + 1;
            }
        "#);
    assert_eq!(code, 31);
}

#[test]
fn test_array_methods() {
    let (_, _, code) = run(r#"
            int main() {
                let a = [1, 2];
                a.push(3);
                a.insert(0, 0);
                let r = a.remove(1);
                let p = a.pop();
                let b = a.slice(0, 2);
                a.extend([7, 8]);
                return a.len() * 50 + b.len() * 20 + r * 10 + p;
            }
        "#);
    assert_eq!(code, 253);
}

#[test]
fn test_extend_self() {
    let (_, _, code) = run(r#"
            int main() {
                let a = [1, 2, 3];
                a.extend(a);
                return a.len() * 10 + a[3];
            }
        "#);
    assert_eq!(code, 61);
}

#[test]
fn test_array_panics() {
    let (_, stderr, code) = run("int main() { let a = [1]; return a[1]; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run("int main() { let a = [1]; a[-1] = 0; return 0; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run("int main() { let a = new int[0]; return a.pop(); }");
    assert!(stderr.contains("pop from empty array"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run("int main() { let a = [1]; a.insert(3, 9); return 0; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run("int main() { let n = 0 - 1; let a = new int[n]; return 0; }");
    assert!(stderr.contains("negative array length"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_concat_is_pure_and_copy_is_independent() {
    let (_, _, code) = run(r#"
            int main() {
                let a = [1, 2];
                let b = [3];
                let c = a + b;
                let d = copy(a);
                d[0] = 9;
                return c.len() * 10 + a.len() * 5 + a[0] + c[2];
            }
        "#);
    assert_eq!(code, 44);
}

#[test]
fn test_structural_equality() {
    let (_, _, code) = run(r#"
            int main() {
                let a = [[1, 2], [3]];
                let b = [[1, 2], [3]];
                let c = [[1, 2], [4]];
                let r = 0;
                if (a == b) { r = r + 1; }
                if (a != c) { r = r + 2; }
                if ([1.5] == [1.5]) { r = r + 4; }
                if ("abc" == "abc") { r = r + 8; }
                if ("abc" != "abd") { r = r + 16; }
                return r;
            }
        "#);
    assert_eq!(code, 31);
}

#[test]
fn test_strings() {
    let (stdout, _, code) = run(r#"
            import "std/io";
            int main() {
                let s = "hello";
                s[0] = 'H';
                io.print(s);
                let t = s + " world";
                io.print(t);
                io.print(t.slice(6, 11));
                return t.len();
            }
        "#);
    assert_eq!(stdout, "Hello\nHello world\nworld\n");
    assert_eq!(code, 11);
}

#[test]
fn test_new_arrays_zeroed_and_struct_slots_distinct() {
    let (_, _, code) = run(r#"
            struct P { x: int }
            int main() {
                let a = new int[3];
                let b = new P[2];
                b[0].x = 7;
                return a[0] + a[1] + a[2] + b[0].x * 10 + b[1].x;
            }
        "#);
    assert_eq!(code, 70);
}

#[test]
fn test_struct_array_members_default_empty() {
    let (_, _, code) = run(r#"
            struct Bag { items: [int], name: string }
            int main() {
                let b = new Bag;
                b.items.push(5);
                b.items.push(6);
                return b.items.len() * 10 + b.items[1] + b.name.len();
            }
        "#);
    assert_eq!(code, 26);
}

#[test]
fn test_char_array_iteration() {
    let (_, _, code) = run(r#"
            int main() {
                let s = "abc";
                let sum = 0;
                for (let i = 0; i < s.len(); i = i + 1) {
                    sum = sum + (s[i] as int);
                }
                return sum - 97 - 98 - 99;
            }
        "#);
    assert_eq!(code, 0);
}

#[test]
fn test_scalar_optionals() {
    let (_, _, code) = run(r#"
            int? find(xs: [int], want: int) {
                for (let i = 0; i < xs.len(); i = i + 1) {
                    if (xs[i] == want) { return i; }
                }
                return none;
            }
            int main() {
                let hit = find([5, 7, 9], 7);
                let miss = find([5, 7, 9], 8);
                let r = 0;
                if (hit != none) { r = r + 1; }
                if (miss == none) { r = r + 2; }
                if (hit! == 1) { r = r + 4; }
                let x: int? = 40;
                let y: int? = x;
                if (x == y) { r = r + 8; }
                if (x != none) { r = r + x!; }
                return r;
            }
        "#);
    assert_eq!(code, 55);
}

#[test]
fn test_optional_coercion_sites() {
    let (_, _, code) = run(r#"
            struct Slot { value: int? }
            int? id(x: int?) { return x; }
            int main() {
                let a: int? = 3;
                a = 4;
                a = none;
                a = 5;
                let s = new Slot { value: 6 };
                s.value = 7;
                let xs: [int?] = [a, none, s.value];
                xs.push(8);
                xs.insert(0, none);
                let sum = 0;
                for (let i = 0; i < xs.len(); i = i + 1) {
                    if (xs[i] != none) { sum = sum + xs[i]!; }
                }
                return sum + id(20)!;
            }
        "#);
    assert_eq!(code, 40);
}

#[test]
fn test_unwrap_none_panics() {
    let (_, stderr, code) = run(r#"
            int main() {
                let x: int? = none;
                return x!;
            }
        "#);
    assert!(stderr.contains("force-unwrapped a none value"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_optional_structs_linked_list() {
    let (_, _, code) = run(r#"
            struct Node { value: int, next: Node? }
            int main() {
                let head = new Node { value: 1, next: new Node { value: 2, next: none } };
                let fresh = new Node;
                let r = 0;
                if (fresh.next == none) { r = r + 1; }
                let sum = 0;
                let cur: Node? = head;
                while (cur != none) {
                    sum = sum + cur!.value;
                    cur = cur!.next;
                }
                return r * 10 + sum;
            }
        "#);
    assert_eq!(code, 13);
}

#[test]
fn test_optional_equality_forms() {
    let (_, _, code) = run(r#"
            int main() {
                let a: int? = 5;
                let b: int? = 5;
                let c: int? = 6;
                let d: int? = none;
                let e: int? = none;
                let r = 0;
                if (a == b) { r = r + 1; }
                if (a != c) { r = r + 2; }
                if (a != d) { r = r + 4; }
                if (d == e) { r = r + 8; }
                if (a == 5) { r = r + 16; }
                let f: real? = 1.5;
                if (f == 1.5) { r = r + 32; }
                let g: [int]? = [1, 2];
                let h: [int]? = [1, 2];
                let i: [int]? = none;
                if (g == h) { r = r + 64; }
                if (g != i) { r = r + 128; }
                return r;
            }
        "#);
    assert_eq!(code, 255);
}

#[test]
fn test_input_reads_lines_until_eof() {
    let long = "x".repeat(9000);
    let (stdout, _, code) = run_program_with_stdin(
        &[(
            "main.kora",
            r#"
                import "std/io";
                int main() {
                    let count = 0;
                    let line = io.input();
                    while (line != none) {
                        io.print(line!);
                        count = count + 1;
                        line = io.input();
                    }
                    return count;
                }
            "#,
        )],
        format!("alpha\n{long}\nbeta").as_bytes(),
    );
    assert_eq!(stdout, format!("alpha\n{long}\nbeta\n"));
    assert_eq!(code, 3);
}

#[test]
fn test_multi_module() {
    let (_, _, code) = run_program(&[
        (
            "main.kora",
            r#"
                import "util.kora";
                import "geo.kora" g;
                int main() {
                    let p = g.origin();
                    p.shift(util.double(3));
                    return util.double(p.x) + g.taxi(p);
                }
            "#,
        ),
        ("util.kora", "int double(x: int) { return x * 2; }"),
        (
            "geo.kora",
            r#"
                import "util.kora";
                struct Point { x: int, y: int }
                impl Point {
                    void shift(self, d: int) { self.x = self.x + d; self.y = self.y + d; }
                }
                Point origin() { return new Point; }
                int taxi(p: Point) { return util.double(p.x + p.y); }
            "#,
        ),
    ]);
    // p = (6, 6); double(6) + taxi = 12 + 24
    assert_eq!(code, 36);
}

#[test]
fn test_std_conv() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            int main() {
                io.print(conv.int_to_string(-42));
                io.print(conv.bool_to_string(1 < 2));
                io.print(conv.real_to_string(3.5));
                io.print(conv.real_to_string(-2.25));
                io.print(conv.real_to_string(2.0));
                let n = conv.string_to_int("123");
                let bad = conv.string_to_int("12x");
                let r = 0;
                if (bad == none) { r = r + 1; }
                if (n != none) { r = r + n! - 123; }
                return r;
            }
        "#);
    assert_eq!(stdout, "-42\ntrue\n3.5\n-2.25\n2.0\n");
    assert_eq!(code, 1);
}

#[test]
fn test_std_str() {
    let (stdout, _, code) = run(r#"
            import "std/str";
            import "std/io";
            int main() {
                let parts = str.split("a,b,c", ',');
                io.print(str.join(parts, "-"));
                io.print(str.to_upper(str.trim("  hi  ")));
                let r = 0;
                if (str.contains("hello", "ell")) { r = r + 1; }
                if (str.starts_with("hello", "he")) { r = r + 2; }
                let i = str.index_of("hello", "llo");
                if (i != none && i! == 2) { r = r + 4; }
                return r + parts.len();
            }
        "#);
    assert_eq!(stdout, "a-b-c\nHI\n");
    assert_eq!(code, 10);
}

#[test]
fn test_std_math() {
    let (_, _, code) = run(r#"
            import "std/math";
            int main() {
                let r = 0;
                if (math.abs(0 - 5) == 5 && math.max(2, 3) == 3) { r = r + 1; }
                if (math.gcd(12, 18) == 6 && math.pow(2, 10) == 1024) { r = r + 2; }
                if (math.absf(math.sqrtf(2.0) - 1.4142135624) < 0.000001) { r = r + 4; }
                if (math.absf(math.sin(1.0) - 0.8414709848) < 0.000001) { r = r + 8; }
                if (math.absf(math.atan2(1.0, 1.0) * 4.0 - 3.1415926536) < 0.000001) { r = r + 16; }
                if (math.absf(math.powf(2.0, 10.0) - 1024.0) < 0.000001) { r = r + 32; }
                if (math.absf(math.exp(1.0) - 2.7182818285) < 0.000001) { r = r + 64; }
                if (math.absf(math.log(math.exp(3.0)) - 3.0) < 0.000001) { r = r + 128; }
                if (math.floorf(2.7) == 2.0 && math.ceilf(2.3) == 3.0) { r = r + 256; }
                if (math.roundf(-2.5) == -3.0 && math.roundf(2.5) == 3.0) { r = r + 512; }
                if (math.absf(math.log2(1024.0) - 10.0) < 0.000001) { r = r + 1024; }
                if (r == 2047) { return 255; }
                return 0;
            }
        "#);
    assert_eq!(code, 255);
}

#[test]
fn test_for_range_loops() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            struct P { x: int }
            int main() {
                let total = 0;
                for x | [1, 2, 3, 4] {
                    if (x == 2) { continue; }
                    if (x == 4) { break; }
                    total = total + x;
                }
                let nested = 0;
                for row | [[10, 20], [30]] {
                    for v | row {
                        nested = nested + v;
                    }
                }
                for c | "ab" io.write([c]);
                io.write("\n");
                let ps = [new P { x: 1 }, new P { x: 2 }];
                for p | ps {
                    p.x = p.x * 10;
                }
                io.print(conv.int_to_string(ps[0].x + ps[1].x));
                let xs = [1];
                let walked = 0;
                for x | xs {
                    walked = walked + 1;
                    if (xs.len() < 3) { xs.push(0); }
                }
                return total * 50 + nested / 10 + walked;
            }
        "#);
    assert_eq!(stdout, "ab\n30\n");
    assert_eq!(code, 209);
}

#[test]
fn test_std_time_now() {
    let (_, _, code) = run(r#"
            import "std/time";
            int main() {
                let t = time.now();
                if (t > 1500000000) { return 1; }
                return 0;
            }
        "#);
    assert_eq!(code, 1);
}

#[test]
fn test_fs_round_trip() {
    let (_, stderr, code) = run(r#"
            import "std/fs";
            int main() {
                let path = "/tmp/kora_e2e_fs_test.txt";
                let r = 0;
                let w = fs.open(path, "w");
                if (w == none) { return 0; }
                w!.write("alpha
beta
");
                w!.close();

                let f = fs.open(path, "r");
                if (f == none) { return 0; }
                if (f!.read_line() == "alpha") { r = r + 1; }
                if (f!.tell() == 6) { r = r + 2; }
                f!.seek(0);
                if (f!.read_all() == "alpha
beta
") { r = r + 4; }
                if (f!.read_char() == none) { r = r + 8; }
                f!.close();

                if (fs.remove(path)) { r = r + 16; }
                if (fs.open(path, "r") == none) { r = r + 32; }
                if (fs.remove(path) == false) { r = r + 64; }
                return r;
            }
        "#);
    assert_eq!(code, 127, "{stderr}");
}

#[test]
fn test_proc_run() {
    let (_, stderr, code) = run(r#"
            import "std/proc";
            int main() {
                let r = 0;
                if (proc.run("exit 7") == 7) { r = r + 1; }
                if (proc.run("true") == 0) { r = r + 2; }
                return r;
            }
        "#);
    assert_eq!(code, 3, "{stderr}");
}

#[test]
fn test_method_symbols_cannot_collide_with_module_functions() {
    let (_, _, code) = run_program(&[
        (
            "main.kora",
            r#"
                import "util.kora" u;
                struct util { x: int }
                impl util { int double(self) { return self.x + 1; } }
                int main() {
                    let s = new util { x: 1 };
                    return s.double() * 10 + u.double(3);
                }
            "#,
        ),
        ("util.kora", "int double(x: int) { return x * 2; }"),
    ]);
    assert_eq!(code, 26);
}

#[test]
fn test_diamond_imports() {
    let (_, _, code) = run_program(&[
        (
            "main.kora",
            r#"
                import "a.kora";
                import "b.kora";
                import "shared.kora";
                int main() { return a.f() + b.g() + shared.base(); }
            "#,
        ),
        (
            "a.kora",
            "import \"shared.kora\";\nint f() { return shared.base() + 1; }",
        ),
        (
            "b.kora",
            "import \"shared.kora\";\nint g() { return shared.base() + 2; }",
        ),
        ("shared.kora", "int base() { return 10; }"),
    ]);
    assert_eq!(code, 33);
}

#[test]
fn test_clear_sleep_random() {
    let start = std::time::Instant::now();
    let (stdout, _, code) = run(r#"
            import "std/term";
            import "std/io";
            import "std/math";
            import "std/time";
            int main() {
                term.clear();
                io.print("fresh");
                time.sleep(50);
                let a = math.random();
                let b = math.random();
                let r = 0;
                if (a >= 0.0 && a < 1.0) { r = r + 1; }
                if (b >= 0.0 && b < 1.0) { r = r + 2; }
                if (a != b) { r = r + 4; }
                return r;
            }
        "#);
    assert_eq!(stdout, "\x1b[2J\x1b[Hfresh\n");
    assert_eq!(code, 7);
    assert!(start.elapsed() >= std::time::Duration::from_millis(40));
}

#[test]
fn test_all_std_modules() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            import "std/math";
            import "std/str";
            import "std/term";

            int main() {
                term.home();
                io.print(str.to_upper("kora") + conv.int_to_string(math.max(1, 5)));
                let r = math.random();
                if (r >= 0.0 && r < 1.0) { return 0; }
                return 1;
            }
        "#);
    assert_eq!(stdout, "\x1b[HKORA5\n");
    assert_eq!(code, 0);
}

#[test]
fn test_copy_is_shallow() {
    let (_, _, code) = run(r#"
            struct Bag { items: [int], tag: int }
            int main() {
                let a = [[1, 2], [3]];
                let b = copy(a);
                b[0].push(9);
                let bag = new Bag { items: [5], tag: 1 };
                let dup = copy(bag);
                dup.items.push(6);
                dup.tag = 2;
                let r = 0;
                if (a[0].len() == 3) { r = r + 1; }
                if (b.len() == 2 && b[1][0] == 3) { r = r + 2; }
                if (bag.items.len() == 2) { r = r + 4; }
                if (bag.tag == 1 && dup.tag == 2) { r = r + 8; }
                return r;
            }
        "#);
    // shallow: copied containers are fresh, nested aggregates are shared
    assert_eq!(code, 15);
}

#[test]
fn test_complex_program() {
    let (stdout, stderr, code) = run(r#"
            import "std/conv";
            import "std/io";
            import "std/math";
            import "std/str";
            struct Vec2 { x: int, y: int }
            impl Vec2 {
                int dot(self, o: Vec2) { return self.x * o.x + self.y * o.y; }
            }
            void show(n: int) { io.print(conv.int_to_string(n)); }
            int main() {
                show(17 / 5 * 100 + 17 % 5);
                show((1 << 40) % 1000007);
                show((12 & 10) * 100 + (12 | 10) + (5 ^ 3));
                show((2.9 as int) * 10 + ('a' as int));
                io.print(str.to_upper("kora") + "-" + str.join(str.split("a,b", ','), "+"));
                let xs = [3, 1, 2];
                xs.insert(0, 9);
                xs.push(xs.remove(1));
                io.print(conv.int_to_string(xs.pop()) + conv.int_to_string(xs.len()));
                let ys = copy(xs);
                ys.extend(ys);
                show(ys.len() * 10 + xs.len());
                if (xs + ys != xs) { io.print("concat-ne"); }
                let v = new Vec2 { x: 3, y: 4 };
                show(v.dot(copy(v)));
                let maybe = conv.string_to_int("123");
                if (maybe != none) { show(maybe! + 1); }
                if (conv.string_to_int("12x") == none) { io.print("bad-none"); }
                io.print(conv.bool_to_string(math.absf(math.sin(1.0) - 0.8414709848) < 0.000001));
                show(math.gcd(84, 35) * 1000 + math.pow(3, 7));
                return 0;
            }
        "#);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.is_empty());
}

#[test]
fn test_kora_names_cannot_interpose_host_symbols() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            int write(x: int) { return x * 2; }
            int malloc(n: int) { return n + 1; }
            int remove(a: int) { return a - 1; }
            int main() {
                let xs = [1, 2, 3];
                xs.push(write(4));
                io.print(conv.int_to_string(malloc(10) + remove(5) + xs[3]));
                return 0;
            }
        "#);
    assert_eq!(stdout, "23\n");
    assert_eq!(code, 0);
}

#[test]
fn test_opaque_defaults_and_none() {
    let (_, _, code) = run(r#"
            struct Handles { h: opaque, m: opaque? }
            int main() {
                let s = new Handles;
                let t = new Handles;
                let r = 0;
                if (s.m == none) { r = r + 1; }
                if (s.h == t.h) { r = r + 2; }
                let m: opaque? = none;
                if (m == s.m) { r = r + 4; }
                let xs = new opaque[2];
                if (xs[0] == xs[1] && xs == [s.h, t.h]) { r = r + 8; }
                return r;
            }
        "#);
    assert_eq!(code, 15);
}

#[test]
fn test_node_panics_on_missing_extern() {
    let dir = temp_dir();
    let entry = dir.join("main.kora");
    std::fs::write(
        &entry,
        "extern void teleport();\nint main() { teleport(); return 0; }",
    )
    .unwrap();
    let emitted = Command::new(env!("CARGO_BIN_EXE_kora"))
        .arg("--emit-js")
        .arg(&entry)
        .output()
        .expect("kora --emit-js");
    assert!(emitted.status.success());

    let out = exec(Command::new("node").arg(dir.join("main.js")), b"");
    std::fs::remove_dir_all(&dir).ok();
    assert_ne!(out.2, 0);
    assert!(
        out.1
            .contains("extern 'teleport' is not provided by this host"),
        "{}",
        out.1
    );
}

#[test]
fn test_emit_js_output_runs_under_node() {
    let dir = temp_dir();
    let entry = dir.join("main.kora");
    std::fs::write(
        &entry,
        r#"
            import "std/io";
            int main() {
                let line = io.input();
                if (line != none) { io.print("got: " + line!); }
                return 3;
            }
        "#,
    )
    .unwrap();

    let emitted = Command::new(env!("CARGO_BIN_EXE_kora"))
        .arg("--emit-js")
        .arg("--emit-llvm")
        .arg(&entry)
        .output()
        .expect("kora --emit-js --emit-llvm");
    assert!(emitted.status.success());
    assert!(emitted.stdout.is_empty(), "artifacts are files, not stdout");
    // Both artifacts land next to the input with derived names.
    let js = dir.join("main.js");
    assert!(dir.join("main.ll").exists());

    let out = exec(Command::new("node").arg(&js), b"hello\n");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(out.0, "got: hello\n");
    assert_eq!(out.2, 3);
}
