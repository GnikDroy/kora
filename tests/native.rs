#![cfg(feature = "codegen")]

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use inkwell::context::Context;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn run_native(source: &str) -> (String, String, i32) {
    run_native_program(&[("main.kora", source)])
}

fn run_native_program(files: &[(&str, &str)]) -> (String, String, i32) {
    let dir = std::env::temp_dir().join(format!(
        "kora-native-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();

    for (name, source) in files {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let entry = dir.join(files[0].0);
    let program = kora::compile(entry.to_str().unwrap(), |path: &Path| {
        std::fs::read_to_string(path).ok()
    })
    .expect("front-end");

    let context = Context::create();
    let llvm = kora::codegen::lower(&context, &program).expect("codegen");
    let binary = dir.join("main");
    kora::codegen::link(&llvm, &binary).expect("build");

    let out = Command::new(&binary).output().expect("run");
    std::fs::remove_dir_all(&dir).ok();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn test_native_fib() {
    let (stdout, stderr, code) = run_native(
        r#"
            extern void print_int(x: int);
            int fib(n: int) {
                if (n < 2) { return n; }
                return fib(n - 1) + fib(n - 2);
            }
            int main() {
                for (let i = 0; i < 10; i = i + 1) { print_int(fib(i)); }
                return 0;
            }
        "#,
    );
    assert_eq!(stdout, "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n", "{stderr}");
    assert_eq!(code, 0);
}

#[test]
fn test_native_exit_code() {
    let (_, _, code) = run_native("int main() { return 42; }");
    assert_eq!(code, 42);
}

#[test]
fn test_native_print_externs() {
    let (stdout, _, code) = run_native(
        r#"
            extern void print_real(x: real);
            extern void print_char(c: char);
            extern void print_bool(b: bool);
            int main() {
                print_real(3.5);
                print_char('a');
                print_char('\n');
                print_bool(1 < 2);
                return 0;
            }
        "#,
    );
    assert_eq!(stdout, "3.5\na\ntrue\n");
    assert_eq!(code, 0);
}

#[test]
fn test_native_division_by_zero_panics() {
    let (_, stderr, code) = run_native("int main() { let z = 0; return 1 / z; }");
    assert!(stderr.contains("division by zero"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_native_modulo_by_zero_panics() {
    let (_, stderr, code) = run_native("int main() { let z = 0; return 1 % z; }");
    assert!(stderr.contains("division by zero"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_native_division_still_works() {
    let (_, _, code) = run_native("int main() { return 17 / 3 + 17 % 3; }");
    assert_eq!(code, 7);
}

#[test]
fn test_native_struct_literals_and_aliasing() {
    let (_, _, code) = run_native(
        r#"
            struct Point { x: int, y: int }
            int main() {
                let p = new Point { x: 1, y: 2 };
                let q = p;
                q.x = 10;
                return p.x * 10 + p.y;
            }
        "#,
    );
    assert_eq!(code, 102);
}

#[test]
fn test_native_struct_mixed_member_layout() {
    let (stdout, _, code) = run_native(
        r#"
            extern void print_real(x: real);
            struct Mixed { c: char, r: real, b: bool, n: int }
            int main() {
                let m = new Mixed { c: 'a', r: 2.5, b: true, n: 7 };
                print_real(m.r);
                if (m.b && m.c == 'a') { return m.n; }
                return 0;
            }
        "#,
    );
    assert_eq!(stdout, "2.5\n");
    assert_eq!(code, 7);
}

#[test]
fn test_native_struct_defaults_are_zeroed_and_distinct() {
    let (_, _, code) = run_native(
        r#"
            struct Inner { a: int, b: real }
            struct Outer { i: Inner, n: int }
            int main() {
                let o = new Outer;
                let p = new Outer;
                o.i.a = 7;
                return o.i.a * 10 + p.i.a + o.n;
            }
        "#,
    );
    assert_eq!(code, 70);
}

#[test]
fn test_native_methods() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 42);
}

#[test]
fn test_native_copy_struct_is_shallow_and_independent() {
    let (_, _, code) = run_native(
        r#"
            struct P { x: int }
            int main() {
                let p = new P { x: 5 };
                let q = copy(p);
                q.x = 9;
                return p.x * 10 + q.x;
            }
        "#,
    );
    assert_eq!(code, 59);
}

#[test]
fn test_native_structs_as_function_values() {
    let (_, _, code) = run_native(
        r#"
            struct Vec2 { x: int, y: int }
            int dot(a: Vec2, b: Vec2) { return a.x * b.x + a.y * b.y; }
            Vec2 make(x: int, y: int) { return new Vec2 { x: x, y: y }; }
            int main() {
                return dot(make(1, 2), make(3, 4));
            }
        "#,
    );
    assert_eq!(code, 11);
}

#[test]
fn test_native_array_literals_index_len() {
    let (_, _, code) = run_native(
        r#"
            int main() {
                let a = [10, 20, 30];
                a[1] = a[0] + 5;
                let x = 0;
                x = a[2] = 99;
                return a.len() * 10 + a[1] + x - 99 - 15 + 1;
            }
        "#,
    );
    assert_eq!(code, 31);
}

#[test]
fn test_native_array_methods() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 253);
}

#[test]
fn test_native_extend_self() {
    let (_, _, code) = run_native(
        r#"
            int main() {
                let a = [1, 2, 3];
                a.extend(a);
                return a.len() * 10 + a[3];
            }
        "#,
    );
    assert_eq!(code, 61);
}

#[test]
fn test_native_array_panics() {
    let (_, stderr, code) = run_native("int main() { let a = [1]; return a[1]; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run_native("int main() { let a = [1]; a[-1] = 0; return 0; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run_native("int main() { let a = new int[0]; return a.pop(); }");
    assert!(stderr.contains("pop from empty array"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run_native("int main() { let a = [1]; a.insert(3, 9); return 0; }");
    assert!(stderr.contains("index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) =
        run_native("int main() { let n = 0 - 1; let a = new int[n]; return 0; }");
    assert!(stderr.contains("negative array length"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_native_concat_is_pure_and_copy_is_independent() {
    let (_, _, code) = run_native(
        r#"
            int main() {
                let a = [1, 2];
                let b = [3];
                let c = a + b;
                let d = copy(a);
                d[0] = 9;
                return c.len() * 10 + a.len() * 5 + a[0] + c[2];
            }
        "#,
    );
    assert_eq!(code, 44);
}

#[test]
fn test_native_structural_equality() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 31);
}

#[test]
fn test_native_strings() {
    let (stdout, _, code) = run_native(
        r#"
            extern void print(s: string);
            int main() {
                let s = "hello";
                s[0] = 'H';
                print(s);
                let t = s + " world";
                print(t);
                print(t.slice(6, 11));
                return t.len();
            }
        "#,
    );
    assert_eq!(stdout, "Hello\nHello world\nworld\n");
    assert_eq!(code, 11);
}

#[test]
fn test_native_new_arrays_zeroed_and_struct_slots_distinct() {
    let (_, _, code) = run_native(
        r#"
            struct P { x: int }
            int main() {
                let a = new int[3];
                let b = new P[2];
                b[0].x = 7;
                return a[0] + a[1] + a[2] + b[0].x * 10 + b[1].x;
            }
        "#,
    );
    assert_eq!(code, 70);
}

#[test]
fn test_native_struct_array_members_default_empty() {
    let (_, _, code) = run_native(
        r#"
            struct Bag { items: [int], name: string }
            int main() {
                let b = new Bag;
                b.items.push(5);
                b.items.push(6);
                return b.items.len() * 10 + b.items[1] + b.name.len();
            }
        "#,
    );
    assert_eq!(code, 26);
}

#[test]
fn test_native_char_array_iteration() {
    let (_, _, code) = run_native(
        r#"
            int main() {
                let s = "abc";
                let sum = 0;
                for (let i = 0; i < s.len(); i = i + 1) {
                    sum = sum + (s[i] as int);
                }
                return sum - 97 - 98 - 99;
            }
        "#,
    );
    assert_eq!(code, 0);
}

#[test]
fn test_native_scalar_optionals() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 55);
}

#[test]
fn test_native_optional_coercion_sites() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 40);
}

#[test]
fn test_native_unwrap_none_panics() {
    let (_, stderr, code) = run_native(
        r#"
            int main() {
                let x: int? = none;
                return x!;
            }
        "#,
    );
    assert!(stderr.contains("force-unwrapped a none value"), "{stderr}");
    assert_eq!(code, 1);
}

#[test]
fn test_native_optional_structs_linked_list() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 13);
}

#[test]
fn test_native_optional_equality_forms() {
    let (_, _, code) = run_native(
        r#"
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
        "#,
    );
    assert_eq!(code, 255);
}

#[test]
fn test_native_input_reads_lines_until_eof() {
    let dir = std::env::temp_dir().join(format!("kora-native-{}-input", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.kora");
    std::fs::write(
        &entry,
        r#"
            extern string? input();
            extern void print(s: string);
            int main() {
                let count = 0;
                let line = input();
                while (line != none) {
                    print(line!);
                    count = count + 1;
                    line = input();
                }
                return count;
            }
        "#,
    )
    .unwrap();
    let program = kora::compile(entry.to_str().unwrap(), |path: &Path| {
        std::fs::read_to_string(path).ok()
    })
    .expect("front-end");
    let context = Context::create();
    let llvm = kora::codegen::lower(&context, &program).expect("codegen");
    let binary = dir.join("main");
    kora::codegen::link(&llvm, &binary).expect("build");

    let mut child = Command::new(&binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run");
    use std::io::Write;
    let long = "x".repeat(9000);
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("alpha\n{long}\nbeta").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("alpha\n{long}\nbeta\n")
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn test_native_scalar_optional_extern_is_rejected() {
    let dir = std::env::temp_dir().join(format!("kora-native-{}-extopt", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.kora");
    std::fs::write(&entry, "extern int? bad();\nint main() { return 0; }").unwrap();
    let program = kora::compile(entry.to_str().unwrap(), |path: &Path| {
        std::fs::read_to_string(path).ok()
    })
    .expect("front-end");
    let context = Context::create();
    let err = kora::codegen::lower(&context, &program).unwrap_err();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        err.to_string()
            .contains("scalar optionals cannot cross the extern boundary"),
        "{err}"
    );
}

#[test]
fn test_native_multi_module() {
    let (_, _, code) = run_native_program(&[
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
fn test_native_std_conv() {
    let (stdout, _, code) = run_native_program(&[(
        "main.kora",
        r#"
            import "std/conv";
            extern void print(s: string);
            int main() {
                print(conv.int_to_string(-42));
                print(conv.bool_to_string(1 < 2));
                let n = conv.string_to_int("123");
                let bad = conv.string_to_int("12x");
                let r = 0;
                if (bad == none) { r = r + 1; }
                if (n != none) { r = r + n! - 123; }
                return r;
            }
        "#,
    )]);
    assert_eq!(stdout, "-42\ntrue\n");
    assert_eq!(code, 1);
}

#[test]
fn test_native_std_str() {
    let (stdout, _, code) = run_native_program(&[(
        "main.kora",
        r#"
            import "std/str";
            extern void print(s: string);
            int main() {
                let parts = str.split("a,b,c", ',');
                print(str.join(parts, "-"));
                print(str.to_upper(str.trim("  hi  ")));
                let r = 0;
                if (str.contains("hello", "ell")) { r = r + 1; }
                if (str.starts_with("hello", "he")) { r = r + 2; }
                let i = str.index_of("hello", "llo");
                if (i != none && i! == 2) { r = r + 4; }
                return r + parts.len();
            }
        "#,
    )]);
    assert_eq!(stdout, "a-b-c\nHI\n");
    assert_eq!(code, 10);
}

#[test]
fn test_native_std_math() {
    let (_, _, code) = run_native_program(&[(
        "main.kora",
        r#"
            import "std/math";
            int main() {
                let r = 0;
                if (math.abs(0 - 5) == 5 && math.max(2, 3) == 3) { r = r + 1; }
                if (math.gcd(12, 18) == 6 && math.pow(2, 10) == 1024) { r = r + 2; }
                if (math.absf(math.sqrtf(2.0) - 1.4142135624) < 0.000001) { r = r + 4; }
                if (math.absf(math.sin(1.0) - 0.8414709848) < 0.000001) { r = r + 8; }
                if (math.absf(math.atan2(1.0, 1.0) * 4.0 - 3.1415926536) < 0.000001) { r = r + 16; }
                if (math.absf(math.powf(2.0, 10.0) - 1024.0) < 0.000001) { r = r + 32; }
                return r;
            }
        "#,
    )]);
    assert_eq!(code, 63);
}

#[test]
fn test_native_diamond_imports() {
    let (_, _, code) = run_native_program(&[
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
