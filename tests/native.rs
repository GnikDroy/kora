#![cfg(feature = "codegen")]

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use inkwell::context::Context;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

fn run_native(source: &str) -> (String, String, i32) {
    let dir = std::env::temp_dir().join(format!(
        "kora-native-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let entry = dir.join("main.kora");
    std::fs::write(&entry, source).unwrap();
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
