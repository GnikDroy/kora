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

fn frontend(dir: &Path, files: &[(&str, &str)]) -> kora_compiler::CompiledProgram {
    for (name, source) in files {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let entry = dir.join(files[0].0);
    kora_compiler::compile(entry.to_str().unwrap(), |path: &Path| {
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
    kora_compiler::backend::native(&program, &binary, "2", &[]).expect("build");
    let native = exec(&mut Command::new(&binary), stdin);

    let js = kora_compiler::backend::node_program(program, HashSet::new()).expect("emit js");
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
    run_native_only_stdin(source, b"")
}

fn run_native_only_stdin(source: &str, stdin: &[u8]) -> (String, String, i32) {
    let dir = temp_dir();
    let program = frontend(&dir, &[("main.kora", source)]);
    let binary = dir.join("main");
    kora_compiler::backend::native(&program, &binary, "2", &[]).expect("build");
    let out = exec(&mut Command::new(&binary), stdin);
    std::fs::remove_dir_all(&dir).ok();
    out
}

fn run_native_only_args(source: &str, args: &[&str]) -> (String, String, i32) {
    let dir = temp_dir();
    let program = frontend(&dir, &[("main.kora", source)]);
    let binary = dir.join("main");
    kora_compiler::backend::native(&program, &binary, "2", &[]).expect("build");
    let mut cmd = Command::new(&binary);
    cmd.args(args).current_dir(&dir);
    let out = exec(&mut cmd, b"");
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
#[cfg(not(target_os = "windows"))]
fn test_native_link_passthrough() {
    let dir = temp_dir();

    let csrc = dir.join("ext.c");
    std::fs::write(&csrc, "int kora_ext_answer(void) { return 42; }\n").unwrap();
    let obj = dir.join("ext.o");
    assert!(
        Command::new("cc")
            .arg("-c")
            .arg(&csrc)
            .arg("-o")
            .arg(&obj)
            .status()
            .unwrap()
            .success(),
        "compile ext.c"
    );
    let archive = dir.join("libkoratest.a");
    assert!(
        Command::new("ar")
            .arg("rcs")
            .arg(&archive)
            .arg(&obj)
            .status()
            .unwrap()
            .success(),
        "archive ext.o"
    );

    let program = frontend(
        &dir,
        &[(
            "main.kora",
            r#"
                extern cint kora_ext_answer();
                int main() {
                    if (kora_ext_answer() == 42) { return 42; }
                    return 1;
                }
            "#,
        )],
    );
    let binary = dir.join("main");
    let link_args = vec![format!("-L{}", dir.display()), "-lkoratest".to_string()];
    kora_compiler::backend::native(&program, &binary, "2", &link_args)
        .expect("build with passthrough");
    let (_, stderr, code) = exec(&mut Command::new(&binary), b"");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(code, 42, "{stderr}");
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_native_fn_pointer_callback() {
    let dir = temp_dir();

    let csrc = dir.join("cb.c");
    std::fs::write(
        &csrc,
        "int kora_apply_i(int (*f)(int), int x) { return f(x); }\n\
         long kora_apply_l(long (*f)(long), long x) { return f(x); }\n",
    )
    .unwrap();
    let obj = dir.join("cb.o");
    assert!(
        Command::new("cc")
            .arg("-c")
            .arg(&csrc)
            .arg("-o")
            .arg(&obj)
            .status()
            .unwrap()
            .success(),
        "compile cb.c"
    );
    let archive = dir.join("libkoracb.a");
    assert!(
        Command::new("ar")
            .arg("rcs")
            .arg(&archive)
            .arg(&obj)
            .status()
            .unwrap()
            .success(),
        "archive cb.o"
    );

    let program = frontend(
        &dir,
        &[(
            "main.kora",
            r#"
                extern cint kora_apply_i(f: cint(cint), x: cint);
                extern clong kora_apply_l(f: clong(clong), x: clong);

                int inc(x: int) { return x + 1; }

                int main() {
                    let a = kora_apply_i(inc, 41);
                    let b = kora_apply_l(inc, 41);
                    if (a == 42) {
                        if (b == 42) {
                            return 42;
                        }
                    }
                    return 1;
                }
            "#,
        )],
    );
    let binary = dir.join("main");
    let link_args = vec![format!("-L{}", dir.display()), "-lkoracb".to_string()];
    kora_compiler::backend::native(&program, &binary, "2", &link_args)
        .expect("build with callback");
    let (_, stderr, code) = exec(&mut Command::new(&binary), b"");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_native_threads() {
    let (_, stderr, code) = run_native_only(
        r#"
            import "std/thread";

            struct Shared {
                count: int,
                mutex: Mutex,
            }

            void worker(s: Shared) {
                let i = 0;
                while (i < 1000) {
                    let junk = new char[8];
                    junk[0] = 'x';
                    s.mutex.lock();
                    s.count = s.count + 1;
                    s.mutex.unlock();
                    i = i + 1;
                }
            }

            int main() {
                let m = thread.mutex();
                if (m == none) { return 2; }
                let s = new Shared { count: 0, mutex: m! };
                let threads: [Thread] = [];
                let n = 0;
                while (n < 4) {
                    let t = thread.spawn::<Shared>(worker, s);
                    if (t != none) {
                        threads.push(t!);
                    }
                    n = n + 1;
                }
                let j = 0;
                while (j < threads.len()) {
                    threads[j].join();
                    j = j + 1;
                }
                if (s.count == 4000) {
                    return 42;
                }
                return 1;
            }
        "#,
    );
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_native_udp_loopback() {
    let (_, stderr, code) = run_native_only(
        r#"
            import "std/net";

            int main() {
                let opened = net.bind_udp("127.0.0.1", 0);
                if (opened == none) { return 1; }
                let sock = opened!;

                let bound = sock.local();
                if (bound == none) { return 2; }
                let target = bound!;

                if (sock.send_to("hello", target) < 0) { return 3; }

                sock.set_timeout(1000);
                let received = sock.recv_from(64);
                if (received == none) { return 4; }
                let datagram = received!;
                sock.close();

                if (datagram.data == "hello") { return 42; }
                return 5;
            }
        "#,
    );
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_native_computed_callback_is_rejected() {
    let errors = compile_fails(&[(
        "main.kora",
        r#"
            extern void needs_cb(f: cint(cint));
            int inc(x: int) { return x + 1; }
            int main() {
                let g = inc;
                needs_cb(g);
                return 0;
            }
        "#,
    )]);
    assert!(errors.contains("must be a named function"), "{errors}");
}

#[test]
fn test_native_env_args() {
    let (stdout, stderr, code) = run_native_only_args(
        r#"
            import "std/env";
            import "std/io";
            import "std/conv";
            int main() {
                let a = env.args();
                io.print(conv.int_to_string(a.len()));
                if (a.len() > 1) { io.print(a[1]); }
                return 0;
            }
        "#,
        &["alpha", "beta"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "3\nalpha\n", "{stderr}");
}

#[test]
fn test_native_env_set_get() {
    let (stdout, stderr, code) = run_native_only(
        r#"
            import "std/env";
            import "std/io";
            int main() {
                env.set("KORA_E2E_SET", "hello");
                let v = env.get("KORA_E2E_SET");
                if (v == none) { return 1; }
                io.print(v!);
                env.unset("KORA_E2E_SET");
                if (env.get("KORA_E2E_SET") != none) { return 2; }
                return 0;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "hello\n", "{stderr}");
}

#[test]
fn test_native_fs_dir_ops() {
    let (_, stderr, code) = run_native_only_args(
        r#"
            import "std/fs";
            int main() {
                if (!fs.mkdir("sub")) { return 1; }
                if (!fs.is_dir("sub")) { return 2; }
                let f = fs.open("sub/a.txt", "w");
                if (f == none) { return 3; }
                let file = f!;
                file.write("hi");
                file.close();
                let sz = fs.size("sub/a.txt");
                if (sz == none) { return 4; }
                if (sz! != 2) { return 5; }
                let names = fs.read_dir("sub");
                let found = false;
                let i = 0;
                while (i < names.len()) {
                    if (names[i] == "a.txt") { found = true; }
                    i = i + 1;
                }
                if (!found) { return 6; }
                fs.remove("sub/a.txt");
                if (!fs.rmdir("sub")) { return 7; }
                return 42;
            }
        "#,
        &[],
    );
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_native_proc_capture() {
    let (stdout, stderr, code) = run_native_only(
        r#"
            import "std/proc";
            import "std/io";
            int main() {
                if (proc.pid() <= 0) { return 1; }
                io.write(proc.capture("echo hi"));
                return 0;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "hi\n", "{stderr}");
}

#[test]
fn test_native_time_mono_ns() {
    let (_, stderr, code) = run_native_only(
        r#"
            import "std/time";
            int main() {
                let a = time.mono_ns();
                let b = time.mono_ns();
                if (b < a) { return 1; }
                return 0;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn test_native_io_is_tty() {
    let (stdout, stderr, code) = run_native_only(
        r#"
            import "std/io";
            import "std/conv";
            int main() {
                io.print(conv.bool_to_string(io.is_tty()));
                return 0;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "false\n", "{stderr}");
}

#[test]
fn test_native_strerror() {
    let (stdout, stderr, code) = run_native_only(
        r#"
            import "std/libc";
            import "std/io";
            int main() {
                io.print(libc.strerror(2));
                return 0;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.trim().is_empty(), "{stderr}");
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
                if (math.absf(math.hypot(3.0, 4.0) - 5.0) < 0.000001
                    && math.absf(math.cbrt(27.0) - 3.0) < 0.000001) { r = r + 2048; }
                if (math.fmod(7.5, 2.0) == 1.5 && math.truncf(-2.7) == -2.0) { r = r + 4096; }
                if (math.absf(math.log10(1000.0) - 3.0) < 0.000001) { r = r + 8192; }
                if (math.absf(math.asin(math.sin(0.5)) - 0.5) < 0.000001) { r = r + 16384; }
                if (math.absf(math.tanh(0.0)) < 0.000001 && math.cosh(0.0) == 1.0) { r = r + 32768; }
                if (r == 65535) { return 255; }
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
    let path = std::env::temp_dir()
        .join(format!("kora_e2e_fs_test_{}.txt", std::process::id()))
        .to_string_lossy()
        .replace('\\', "/");
    let (_, stderr, code) = run(&format!(
        r#"
            import "std/fs";
            int main() {{
                let path = "{path}";
                let r = 0;
                let w = fs.open(path, "wb");
                if (w == none) {{ return 0; }}
                w!.write("alpha
beta
");
                w!.close();

                let f = fs.open(path, "rb");
                if (f == none) {{ return 0; }}
                if (f!.read_line() == "alpha") {{ r = r + 1; }}
                if (f!.tell() == 6) {{ r = r + 2; }}
                f!.seek(0);
                if (f!.read_all() == "alpha
beta
") {{ r = r + 4; }}
                if (f!.read_char() == none) {{ r = r + 8; }}
                f!.close();

                if (fs.remove(path)) {{ r = r + 16; }}
                if (fs.open(path, "rb") == none) {{ r = r + 32; }}
                if (fs.remove(path) == false) {{ r = r + 64; }}
                return r;
            }}
        "#
    ));
    assert_eq!(code, 127, "{stderr}");
}

#[test]
fn test_proc_run() {
    let (_, stderr, code) = run(r#"
            import "std/proc";
            int main() {
                let r = 0;
                if (proc.run("exit 7") == 7) { r = r + 1; }
                if (proc.run("exit 0") == 0) { r = r + 2; }
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
fn test_std_term_raw_mode_and_key_reads() {
    let (_, stderr, code) = run_native_only_stdin(
        r#"
            import "std/term";
            int main() {
                if (term.raw(true)) { return 1; }
                if (!term.raw(false)) { return 2; }
                let a = term.read_key(5000);
                if (a == none) { return 3; }
                if (a! != 'a') { return 4; }
                let b = term.read_key(5000);
                if (b == none) { return 5; }
                if (b! != 'b') { return 6; }
                if (term.read_key(50) != none) { return 7; }
                if (term.cols() != none) { return 8; }
                if (term.rows() != none) { return 9; }
                return 42;
            }
        "#,
        b"ab",
    );
    assert_eq!(code, 42, "{stderr}");
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
fn test_signed_division_and_modulo_negatives() {
    let (_, _, code) = run(r#"
            int main() {
                let r = 0;
                if ((-7) / 2 == -3) { r = r + 1; }
                if ((-7) % 2 == -1) { r = r + 2; }
                if (7 % (-2) == 1) { r = r + 4; }
                if ((-7) % (-2) == -1) { r = r + 8; }
                if (7 / (-2) == -3) { r = r + 16; }
                return r;
            }
        "#);
    assert_eq!(code, 31);
}

#[test]
fn test_optional_payload_shapes() {
    let (_, _, code) = run(r#"
            struct Multi { a: int?, b: real?, c: char?, d: bool? }
            int main() {
                let m = new Multi;
                let r = 0;
                if (m.a == none && m.b == none && m.c == none && m.d == none) { r = r + 1; }
                m.a = 5;
                m.b = 2.5;
                m.c = 'x';
                m.d = true;
                if (m.a! == 5 && m.b! == 2.5 && m.c! == 'x' && m.d!) { r = r + 2; }
                m.d = false;
                if (m.d != none && !m.d!) { r = r + 4; }
                let br: real? = none;
                br = 1.25;
                if (br == 1.25) { r = r + 8; }
                let bc: char? = 'q';
                if (bc != none && bc! == 'q') { r = r + 16; }
                let bb: bool? = false;
                if (bb != none && bb == false) { r = r + 32; }
                let xs: [int?] = [1, none, 3];
                xs[0] = none;
                xs[1] = 2;
                if (xs[0] == none && xs[1]! == 2 && xs[2]! == 3) { r = r + 64; }
                return r;
            }
        "#);
    assert_eq!(code, 127);
}

#[test]
fn test_real_special_values() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            int main() {
                let z = 0.0;
                let inf = 1.0 / z;
                let nan = z / z;
                let negz = -1.0 * 0.0;
                let r = 0;
                if (inf > 1000000000000.0) { r = r + 1; }
                if (0.0 - inf < 0.0 - 1000000000000.0) { r = r + 2; }
                if (nan != nan) { r = r + 4; }
                if (!(nan == nan)) { r = r + 8; }
                if (negz == 0.0) { r = r + 16; }
                if (1.0 / negz < 0.0) { r = r + 32; }
                if (inf == inf) { r = r + 64; }
                io.print(conv.real_to_string(inf));
                io.print(conv.real_to_string(0.0 - inf));
                io.print(conv.real_to_string(nan));
                return r;
            }
        "#);
    assert_eq!(stdout, "inf\n-inf\nnan\n");
    assert_eq!(code, 127);
}

#[test]
fn test_bool_equality_and_chained_short_circuit() {
    let (stdout, _, code) = run(r#"
            import "std/conv";
            import "std/io";
            bool step(log: [int], id: int, v: bool) { log.push(id); return v; }
            int main() {
                let r = 0;
                if (true == true && true != false) { r = r + 1; }
                if ((1 < 2) == true) { r = r + 2; }
                let log = new int[0];
                if (step(log, 1, true) && step(log, 2, false) && step(log, 3, true)) { r = r + 100; }
                if (step(log, 4, false) || step(log, 5, true) || step(log, 6, true)) { r = r + 4; }
                let s = "";
                for i | log { s = s + conv.int_to_string(i); }
                io.print(s);
                if (log.len() == 4) { r = r + 8; }
                return r;
            }
        "#);
    assert_eq!(stdout, "1245\n");
    assert_eq!(code, 15);
}

#[test]
fn test_mutual_recursion() {
    let (_, _, code) = run(r#"
            bool is_even(n: int) {
                if (n == 0) { return true; }
                return is_odd(n - 1);
            }
            bool is_odd(n: int) {
                if (n == 0) { return false; }
                return is_even(n - 1);
            }
            int main() {
                let r = 0;
                if (is_even(100)) { r = r + 1; }
                if (is_odd(77)) { r = r + 2; }
                if (!is_even(1)) { r = r + 4; }
                return r;
            }
        "#);
    assert_eq!(code, 7);
}

#[test]
fn test_empty_string_and_growth() {
    let (stdout, _, code) = run(r#"
            import "std/io";
            int main() {
                let s = "";
                let r = 0;
                if (s.len() == 0) { r = r + 1; }
                io.print(s);
                let t = s + "x" + "";
                if (t == "x" && t.len() == 1) { r = r + 2; }
                let xs = new int[0];
                for (let i = 0; i < 10000; i = i + 1) { xs.push(i); }
                if (xs.len() == 10000 && xs[0] == 0 && xs[1234] == 1234 && xs[9999] == 9999) { r = r + 4; }
                return r;
            }
        "#);
    assert_eq!(stdout, "\n");
    assert_eq!(code, 7);
}

#[cfg(unix)]
#[test]
fn test_negative_and_large_exit_codes() {
    let (_, _, code) = run("int main() { return -1; }");
    assert_eq!(code, 255);
    let (_, _, code) = run("int main() { return 300; }");
    assert_eq!(code, 44);
}

#[test]
fn test_deep_early_return() {
    let (_, _, code) = run(r#"
            int find(grid: [[int]], want: int) {
                for (let i = 0; i < grid.len(); i = i + 1) {
                    for (let j = 0; j < grid[i].len(); j = j + 1) {
                        if (grid[i][j] == want) {
                            while (true) {
                                return i * 10 + j;
                            }
                        }
                    }
                }
                return -1;
            }
            int main() {
                let inner_breaks = 0;
                for (let i = 0; i < 3; i = i + 1) {
                    for (let j = 0; j < 10; j = j + 1) {
                        if (j == 1) { break; }
                        inner_breaks = inner_breaks + 1;
                    }
                }
                let grid = [[1, 2], [3, 4]];
                return find(grid, 4) * 10 + inner_breaks + find(grid, 9) + 1;
            }
        "#);
    assert_eq!(code, 113);
}

#[test]
fn test_gc_survives_allocation_churn() {
    let (_, stderr, code) = run_native_only(
        r#"
            struct Blob { data: [int], tag: int }
            int main() {
                let drift = 0;
                for (let i = 0; i < 200000; i = i + 1) {
                    let b = new Blob { data: new int[64], tag: i };
                    b.data[63] = i;
                    drift = drift + b.data[63] - b.tag;
                }
                return drift;
            }
        "#,
    );
    assert_eq!(code, 0, "{stderr}");
}

#[test]
fn test_generic_identity_and_pair() {
    let (stdout, stderr, code) = run(r#"
            import "std/conv";
            import "std/io";
            struct pair<A, B> { first: A, second: B }
            impl pair<A, B> {
                A fst(self) { return self.first; }
                void set_second(self, v: B) { self.second = v; }
            }
            T id<T>(x: T) { return x; }
            int main() {
                let p = new pair<int, string>{ first: 40, second: "xy" };
                p.set_second("abc");
                io.print(conv.int_to_string(id::<int>(p.fst()) + p.second.len()));
                let q = new pair<bool, real>{ first: true, second: 1.5 };
                if (q.fst() && q.second == 1.5) { return id::<int>(2); }
                return 0;
            }
        "#);
    assert_eq!(stdout, "43\n", "{stderr}");
    assert_eq!(code, 2);
}

#[test]
fn test_generic_struct_methods_monomorphize() {
    let (_, stderr, code) = run(r#"
            struct box<T> { v: T }
            impl box<T> { T get(self) { return self.v; } }
            int main() {
                let a = new box<int>{ v: 41 };
                let b = new box<bool>{ v: true };
                if (b.get()) { return a.get() + 1; }
                return 0;
            }
        "#);
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_generic_across_modules() {
    let (_, stderr, code) = run_program(&[
        (
            "main.kora",
            r#"
                import "second.kora";
                import "util.kora";
                int main() {
                    let b = util.make::<int>(35);
                    return b.v + second.get();
                }
            "#,
        ),
        (
            "second.kora",
            r#"
                import "util.kora";
                int get() {
                    let b = util.make::<int>(7);
                    return b.v;
                }
            "#,
        ),
        (
            "util.kora",
            r#"
                struct box<T> { v: T }
                box<T> make<T>(v: T) { return new box<T>{ v: v }; }
            "#,
        ),
    ]);
    assert_eq!(code, 42, "{stderr}");
}

#[test]
fn test_generic_list_pattern() {
    let (stdout, stderr, code) = run(r#"
            import "std/conv";
            import "std/io";
            struct node<T> { value: T, next: node<T>? }
            node<int> cons(v: int, rest: node<int>?) {
                return new node<int>{ value: v, next: rest };
            }
            int main() {
                let list: node<int>? = cons(3, cons(2, cons(1, none)));
                let sum = 0;
                let cur = list;
                while (cur != none) {
                    sum = sum + cur!.value;
                    io.print(conv.int_to_string(cur!.value));
                    cur = cur!.next;
                }
                return sum;
            }
        "#);
    assert_eq!(stdout, "3\n2\n1\n", "{stderr}");
    assert_eq!(code, 6);
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

fn compile_fails(files: &[(&str, &str)]) -> String {
    let dir = temp_dir();
    for (name, source) in files {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let entry = dir.join(files[0].0);
    let result = kora_compiler::compile(entry.to_str().unwrap(), |path: &Path| {
        std::fs::read_to_string(path).ok()
    });
    std::fs::remove_dir_all(&dir).ok();
    let errors = match result {
        Ok(_) => panic!("expected a compile error"),
        Err(errors) => errors,
    };
    errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_generic_deeply_nested_boxes() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let a = new box<int>{ v: 7 };
            let b = new box<box<int>>{ v: a };
            let c = new box<box<box<int>>>{ v: b };
            if (c.get().get().get() != 7) { return 1; }
            if (b.get().get() != 7) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_distinct_scalar_instantiations() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let i = new box<int>{ v: 5 };
            let b = new box<bool>{ v: true };
            let c = new box<char>{ v: 'z' };
            let r = 0;
            if (i.get() == 5) { r = r + 1; }
            if (b.get()) { r = r + 1; }
            if (c.get() == 'z') { r = r + 1; }
            if (r == 3) { return 42; }
            return r;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_two_type_params_and_swap() {
    let (_, _, code) = run(r#"
        struct pair<A, B> { fst: A, snd: B }
        impl pair<A, B> {
            A first(self) { return self.fst; }
            B second(self) { return self.snd; }
            pair<B, A> swap(self) { return new pair<B, A>{ fst: self.snd, snd: self.fst }; }
        }
        int main() {
            let p = new pair<int, char>{ fst: 9, snd: 'x' };
            let q = p.swap();
            if (p.first() != 9) { return 1; }
            if (q.second() != 9) { return 2; }
            if (q.first() != 'x') { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_recursive_linked_list() {
    let (_, _, code) = run(r#"
        struct node<T> { value: T, next: node<T>? }
        struct list<T> { head: node<T>? }
        list<T> make_list<T>() { return new list<T>{ head: none }; }
        impl list<T> {
            void push_front(self, x: T) { self.head = new node<T>{ value: x, next: self.head }; }
            int length(self) {
                let n = 0;
                let cur = self.head;
                while (cur != none) { n = n + 1; cur = cur!.next; }
                return n;
            }
            int sum(self) {
                let s = 0;
                let cur = self.head;
                while (cur != none) { s = s + cur!.value; cur = cur!.next; }
                return s;
            }
        }
        int main() {
            let l = make_list::<int>();
            l.push_front(10); l.push_front(20); l.push_front(12);
            if (l.length() != 3) { return 1; }
            if (l.sum() != 42) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_stack_over_array() {
    let (_, _, code) = run(r#"
        struct stack<T> { items: [T] }
        stack<T> make_stack<T>() { return new stack<T>{ items: [] }; }
        impl stack<T> {
            void push(self, x: T) { self.items.push(x); }
            T pop(self) { return self.items.pop(); }
            int size(self) { return self.items.len(); }
        }
        int main() {
            let s = make_stack::<int>();
            s.push(1); s.push(2); s.push(40);
            if (s.size() != 3) { return 1; }
            let a = s.pop();
            let b = s.pop();
            if (a != 40) { return 2; }
            if (b != 2) { return 3; }
            if (s.size() != 1) { return 4; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_fn_calls_generic_void() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        T ident<T>(x: T) { return x; }
        box<T> wrap<T>(x: T) { return new box<T>{ v: ident::<T>(x) }; }
        T unwrap<T>(b: box<T>) { return b.v; }
        int main() {
            let b = wrap::<int>(21);
            let x = unwrap::<int>(b);
            let y = ident::<int>(x);
            if (x + y != 42) { return 1; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_sort_with_comparator() {
    let (_, _, code) = run(r#"
        struct asc {}
        impl asc { bool less(self, a: int, b: int) { return a < b; } }
        struct desc {}
        impl desc { bool less(self, a: int, b: int) { return a > b; } }
        void sort<T, C>(xs: [T], cmp: C) {
            let n = xs.len();
            let i = 0;
            while (i < n) {
                let j = i + 1;
                while (j < n) {
                    if (cmp.less(xs[j], xs[i])) { let tmp = xs[i]; xs[i] = xs[j]; xs[j] = tmp; }
                    j = j + 1;
                }
                i = i + 1;
            }
        }
        int main() {
            let xs = [5, 2, 8, 1, 9, 3];
            sort::<int, asc>(xs, new asc);
            if (xs[0] != 1) { return 1; }
            if (xs[5] != 9) { return 2; }
            sort::<int, desc>(xs, new desc);
            if (xs[0] != 9) { return 3; }
            if (xs[5] != 1) { return 4; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_fold_with_combiner() {
    let (_, _, code) = run(r#"
        struct adder {}
        impl adder { int combine(self, a: int, b: int) { return a + b; } }
        struct multiplier {}
        impl multiplier { int combine(self, a: int, b: int) { return a * b; } }
        int fold<T, F>(xs: [T], init: int, f: F) {
            let acc = init;
            let i = 0;
            while (i < xs.len()) { acc = f.combine(acc, xs[i]); i = i + 1; }
            return acc;
        }
        int main() {
            let xs = [1, 2, 3, 4, 5];
            let s = fold::<int, adder>(xs, 0, new adder);
            let p = fold::<int, multiplier>(xs, 1, new multiplier);
            if (s != 15) { return 1; }
            if (p != 120) { return 2; }
            if (s + p - 93 != 42) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_instances_as_struct_fields() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        struct holder { a: box<int>, b: box<char> }
        int main() {
            let h = new holder{ a: new box<int>{ v: 30 }, b: new box<char>{ v: 'c' } };
            if (h.a.v != 30) { return 1; }
            if (h.b.v != 'c') { return 2; }
            h.a.v = 42;
            if (h.a.v != 42) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_array_of_generic_instances() {
    let (_, _, code) = run(r#"
        struct pair<A,B> { fst: A, snd: B }
        int main() {
            let ps: [pair<int, char>] = [];
            ps.push(new pair<int, char>{ fst: 10, snd: 'a' });
            ps.push(new pair<int, char>{ fst: 32, snd: 'b' });
            let r = 0;
            let i = 0;
            while (i < ps.len()) { r = r + ps[i].fst; i = i + 1; }
            if (r != 42) { return 1; }
            if (ps[1].snd != 'b') { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_over_generic_instance() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        struct pair<A,B> { fst: A, snd: B }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let p = new pair<int, int>{ fst: 20, snd: 22 };
            let b = new box<pair<int, int>>{ v: p };
            let g = b.get();
            if (g.fst + g.snd != 42) { return 1; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_copy_of_generic_is_independent() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        int main() {
            let a = new box<int>{ v: 10 };
            let b = copy(a);
            b.v = 99;
            if (a.v != 10) { return 1; }
            if (b.v != 99) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_nested_generic_instances_stay_distinct() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let bi = new box<box<int>>{ v: new box<int>{ v: 5 } };
            let bb = new box<box<bool>>{ v: new box<bool>{ v: true } };
            let bc = new box<box<char>>{ v: new box<char>{ v: 'q' } };
            let r = 0;
            if (bi.get().get() == 5) { r = r + 10; }
            if (bb.get().get()) { r = r + 30; }
            if (bc.get().get() == 'q') { r = r + 2; }
            if (r == 42) { return 42; }
            return r;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_bst_with_comparator() {
    let (_, _, code) = run(r#"
        struct tnode<T> { value: T, left: tnode<T>?, right: tnode<T>? }
        struct tree<T, C> { root: tnode<T>?, cmp: C }
        struct intcmp {}
        impl intcmp { int compare(self, a: int, b: int) { if (a < b) { return -1; } if (a > b) { return 1; } return 0; } }
        tree<T, C> make_tree<T, C>() { return new tree<T, C>{ root: none, cmp: new C }; }
        impl tree<T, C> {
            void insert(self, x: T) { self.root = self.insert_at(self.root, x); }
            tnode<T>? insert_at(self, cur: tnode<T>?, x: T) {
                if (cur == none) { return new tnode<T>{ value: x, left: none, right: none }; }
                let c = cur!;
                if (self.cmp.compare(x, c.value) < 0) { c.left = self.insert_at(c.left, x); }
                else { c.right = self.insert_at(c.right, x); }
                return c;
            }
            bool contains(self, x: T) {
                let cur = self.root;
                while (cur != none) {
                    let d = self.cmp.compare(x, cur!.value);
                    if (d == 0) { return true; }
                    if (d < 0) { cur = cur!.left; } else { cur = cur!.right; }
                }
                return false;
            }
        }
        int main() {
            let t = make_tree::<int, intcmp>();
            t.insert(5); t.insert(2); t.insert(8); t.insert(1); t.insert(9);
            let r = 0;
            if (t.contains(8)) { r = r + 40; }
            if (!t.contains(7)) { r = r + 2; }
            if (r == 42) { return 42; }
            return r;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_many_generic_instantiations() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let a = new box<int>{ v: 1 };
            let b = new box<bool>{ v: false };
            let c = new box<char>{ v: 'a' };
            let d = new box<real>{ v: 2.5 };
            let e = new box<string>{ v: "hi" };
            let f = new box<box<int>>{ v: new box<int>{ v: 40 } };
            let r = 0;
            if (a.get() == 1) { r = r + 1; }
            if (!b.get()) { r = r + 1; }
            if (c.get() == 'a') { r = r + 1; }
            if (d.get() > 2.0) { r = r + 1; }
            if (e.get().len() == 2) { r = r + 1; }
            if (f.get().get() == 40) { r = r + 37; }
            if (r == 42) { return 42; }
            return r;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_method_takes_same_generic() {
    let (_, _, code) = run(r#"
        struct node<T> { value: T, next: node<T>? }
        struct list<T> { head: node<T>? }
        list<T> make_list<T>() { return new list<T>{ head: none }; }
        impl list<T> {
            void push_front(self, x: T) { self.head = new node<T>{ value: x, next: self.head }; }
            void append_all(self, other: list<T>) {
                let c = other.head;
                while (c != none) { self.push_front(c!.value); c = c!.next; }
            }
            int count(self) { let n = 0; let c = self.head; while (c != none) { n = n + 1; c = c!.next; } return n; }
        }
        int main() {
            let a = make_list::<int>();
            a.push_front(1); a.push_front(2);
            let b = make_list::<int>();
            b.push_front(3); b.push_front(4); b.push_front(5);
            a.append_all(b);
            if (a.count() != 5) { return 1; }
            if (b.count() != 3) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_comparator_over_ordered_types() {
    let (_, _, code) = run(r#"
        struct less<T> {}
        impl less<T> { bool lt(self, a: T, b: T) { return a < b; } }
        T min_of<T, C>(xs: [T], cmp: C) {
            let m = xs[0];
            let i = 1;
            while (i < xs.len()) { if (cmp.lt(xs[i], m)) { m = xs[i]; } i = i + 1; }
            return m;
        }
        int main() {
            let xs = [7, 3, 9, 1, 5];
            let m = min_of::<int, less<int>>(xs, new less<int>);
            if (m != 1) { return 1; }
            let cs = ['d', 'a', 'z'];
            let mc = min_of::<char, less<char>>(cs, new less<char>);
            if (mc != 'a') { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_phantom_type_parameter() {
    let (_, _, code) = run(r#"
        struct tagged<T> { x: int }
        impl tagged<T> { int val(self) { return self.x; } }
        int main() {
            let a = new tagged<int>{ x: 20 };
            let b = new tagged<bool>{ x: 22 };
            if (a.val() + b.val() != 42) { return 1; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_optional_field() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T? }
        impl box<T> {
            bool has(self) { return self.v != none; }
            T force(self) { return self.v!; }
        }
        int main() {
            let a = new box<int>{ v: 42 };
            let b = new box<int>{ v: none };
            if (!a.has()) { return 1; }
            if (b.has()) { return 2; }
            if (a.force() != 42) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_swap_over_arrays() {
    let (_, _, code) = run(r#"
        void swap<T>(xs: [T], i: int, j: int) { let t = xs[i]; xs[i] = xs[j]; xs[j] = t; }
        int main() {
            let xs = [1, 2, 3];
            swap::<int>(xs, 0, 2);
            if (xs[0] != 3) { return 1; }
            if (xs[2] != 1) { return 2; }
            let ss = ["a", "b"];
            swap::<string>(ss, 0, 1);
            if (ss[0] != "b") { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_cross_module_generic() {
    let (_, _, code) = run_program(&[
        (
            "main.kora",
            r#"
                import "lib.kora";
                int main() {
                    let b = lib.make_box::<int>(42);
                    return b.v;
                }
            "#,
        ),
        (
            "lib.kora",
            "struct box<T> { v: T } box<T> make_box<T>(x: T) { return new box<T>{ v: x }; }",
        ),
    ]);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_runaway_instantiation_is_capped() {
    // An infinitely self-expanding generic must terminate at the depth cap,
    // never hang the compiler.
    let errors = compile_fails(&[(
        "main.kora",
        r#"
            struct w<T> { inner: w<w<T>>? }
            int main() {
                let x = new w<int>{ inner: none };
                return 0;
            }
        "#,
    )]);
    assert!(errors.contains("depth limit"), "{errors}");
}

#[test]
fn test_undefined_type_argument_is_rejected() {
    // The undefined `T` is only reachable through a self-referential field
    // (`P<T>?`), so it never surfaces as a concrete type; the instantiation
    // argument itself must still be validated.
    let errors = compile_fails(&[(
        "main.kora",
        "struct P<T> { node: P<T>? } int main() { let p = new P<T>{ node: none }; return 0; }",
    )]);
    assert!(errors.contains("Undefined type"), "{errors}");
}

#[test]
fn test_generic_body_error_reports_instantiation_site() {
    // `<` is scalar-only; instantiating a generic body over string must fail
    // with a note pointing at the instantiation.
    let errors = compile_fails(&[(
        "main.kora",
        r#"
            struct lt<T> {}
            impl lt<T> { bool less(self, a: T, b: T) { return a < b; } }
            int main() {
                let c = new lt<string>;
                if (c.less("a", "b")) { return 0; }
                return 1;
            }
        "#,
    )]);
    assert!(errors.contains("instantiated here"), "{errors}");
}

#[test]
fn test_generic_struct_arity_too_many() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let b = new box<int, bool>{ v: 1 }; return 0; }",
    )]);
    assert!(e.contains("type argument(s), found"), "{e}");
}

#[test]
fn test_generic_struct_arity_too_few() {
    let e = compile_fails(&[(
        "main.kora",
        "struct pair<A, B> { x: A, y: B } int main() { let p = new pair<int>{ x: 1, y: 2 }; return 0; }",
    )]);
    assert!(e.contains("type argument(s), found"), "{e}");
}

#[test]
fn test_generic_turbofish_arity_mismatch() {
    let e = compile_fails(&[(
        "main.kora",
        "T id<T>(x: T) { return x; } int main() { return id::<int, bool>(5); }",
    )]);
    assert!(e.contains("type argument(s), found"), "{e}");
}

#[test]
fn test_generic_duplicate_type_param() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T, T> { v: T } int main() { let b = new box<int, int>{ v: 1 }; return 0; }",
    )]);
    assert!(e.contains("duplicate type parameter"), "{e}");
}

#[test]
fn test_generic_method_level_generics_rejected() {
    // Method-level type parameters are not a v1 feature; the parser rejects them.
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } impl box<T> { U get<U>(self) { return self.v; } } int main() { return 0; }",
    )]);
    assert!(!e.is_empty(), "{e}");
}

#[test]
fn test_generic_nested_optional_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T? } int main() { let b = new box<int?>{ v: none }; return 0; }",
    )]);
    assert!(e.contains("nested optional"), "{e}");
}

#[test]
fn test_generic_missing_duck_typed_method() {
    let e = compile_fails(&[(
        "main.kora",
        "void sort<T, C>(xs: [T], c: C) { if (c.less(xs[0], xs[0])) { let x = 0; } } struct nocmp {} int main() { let xs = [1]; sort::<int, nocmp>(xs, new nocmp); return 0; }",
    )]);
    assert!(e.contains("Invalid member"), "{e}");
}

#[test]
fn test_generic_method_on_scalar_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "T f<T>(x: T) { return x.foo(); } int main() { return f::<int>(5); }",
    )]);
    assert!(e.contains("struct type"), "{e}");
}

#[test]
fn test_generic_undefined_unused_type_arg() {
    // T is unused in fields, yet `box<Undefined>` still validates its argument.
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> {} impl box<T> { int m(self) { return 0; } } int main() { let b = new box<Undefined>; return b.m(); }",
    )]);
    assert!(e.contains("Undefined type"), "{e}");
}

#[test]
fn test_generic_used_without_type_args() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let b: box = new box<int>{ v: 1 }; return 0; }",
    )]);
    assert!(e.contains("requires type arguments"), "{e}");
}

#[test]
fn test_generic_infinite_function_instantiation_capped() {
    // A self-widening generic function must terminate at the depth cap.
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int f<T>() { return f::<box<T>>(); } int main() { return f::<int>(); }",
    )]);
    assert!(e.contains("depth limit"), "{e}");
}

#[test]
fn test_generic_new_array_of_reference_element_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct s<T> { a: [T] } s<T> make<T>() { return new s<T>{ a: new T[3] }; } int main() { let x = make::<string>(); return 0; }",
    )]);
    assert!(e.contains("scalar element"), "{e}");
}

#[test]
fn test_generic_mutual_recursion() {
    let (_, _, code) = run(r#"
        struct A<T> { b: B<T>?, v: T }
        struct B<T> { a: A<T>?, w: T }
        int main() {
            let a = new A<int>{ b: none, v: 20 };
            let b = new B<int>{ a: none, w: 22 };
            a.b = b;
            b.a = a;
            if (a.v + a.b!.w != 42) { return 1; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_five_type_params() {
    let (_, _, code) = run(r#"
        struct five<A,B,C,D,E> { a:A, b:B, c:C, d:D, e:E }
        int main() {
            let x = new five<int,bool,char,int,int>{ a:10, b:true, c:'z', d:20, e:12 };
            if (x.b && x.c == 'z') { return x.a + x.d + x.e; }
            return 0;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_over_array_type() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let b = new box<[int]>{ v: [40, 2] };
            let arr = b.get();
            return arr[0] + arr[1];
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_over_optional_type() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let b = new box<int?>{ v: 42 };
            let o = b.get();
            if (o == none) { return 1; }
            return o!;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_stateful_functor() {
    let (_, _, code) = run(r#"
        struct counter { n: int }
        impl counter { int apply(self, x: int) { self.n = self.n + 1; return x; } }
        int each<T, F>(xs: [T], f: F) {
            let i = 0;
            while (i < xs.len()) { let y = f.apply(xs[i]); i = i + 1; }
            return 0;
        }
        int main() {
            let c = new counter{ n: 0 };
            each::<int, counter>([1,2,3,4,5], c);
            return c.n + 37;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_two_functor_instances_distinct() {
    let (_, _, code) = run(r#"
        struct less<T> {}
        impl less<T> { bool lt(self, a: T, b: T) { return a < b; } }
        bool check<T, C>(a: T, b: T, c: C) { return c.lt(a, b); }
        int main() {
            let r = 0;
            if (check::<int, less<int>>(1, 2, new less<int>)) { r = r + 20; }
            if (check::<char, less<char>>('a', 'b', new less<char>)) { r = r + 22; }
            if (r == 42) { return 42; }
            return r;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_method_constructs_multiple_instances() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        box<T> dup<T>(x: T) {
            let a = new box<T>{ v: x };
            let b = new box<T>{ v: a.get() };
            return b;
        }
        int main() {
            let r = dup::<int>(42);
            return r.get();
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_struct_equality_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let a = new box<int>{v:1}; let b = new box<int>{v:1}; if (a == b) { return 0; } return 1; }",
    )]);
    assert!(e.contains("Binary operator"), "{e}");
}

#[test]
fn test_generic_array_of_struct_equality_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let a: [box<int>] = []; let b: [box<int>] = []; if (a == b) { return 0; } return 1; }",
    )]);
    assert!(e.contains("Binary operator"), "{e}");
}

#[test]
fn test_generic_cross_instantiation_assignment_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let a = new box<int>{v:1}; let b = new box<bool>{v:true}; a = b; return 0; }",
    )]);
    assert!(e.contains("don't match"), "{e}");
}

#[test]
fn test_generic_comparator_type_mismatch_reports_instantiation() {
    let e = compile_fails(&[(
        "main.kora",
        r#"
            struct intcmp {}
            impl intcmp { bool lt(self, a: int, b: int) { return a < b; } }
            bool check<T, C>(a: T, b: T, c: C) { return c.lt(a, b); }
            int main() {
                if (check::<string, intcmp>("a", "b", new intcmp)) { return 0; }
                return 1;
            }
        "#,
    )]);
    assert!(e.contains("instantiated here"), "{e}");
}

#[test]
fn test_generic_int_literal_to_real_field_rejected() {
    // No implicit int->real coercion, even through a generic field.
    let e = compile_fails(&[(
        "main.kora",
        "struct box<T> { v: T } int main() { let b = new box<real>{ v: 2 }; return 0; }",
    )]);
    assert!(e.contains("does not match the member"), "{e}");
}

#[test]
fn test_generic_type_param_shadowing_struct_rejected() {
    let e = compile_fails(&[(
        "main.kora",
        "struct S { n: int } struct box<S> { v: S } int main() { return 0; }",
    )]);
    assert!(e.contains("shadows struct"), "{e}");
}

#[test]
fn test_generic_user_struct_as_type_argument() {
    let (_, _, code) = run(r#"
        struct Point { x: int, y: int }
        struct box<T> { v: T }
        impl box<T> { T get(self) { return self.v; } }
        int main() {
            let p = new Point{ x: 40, y: 2 };
            let b = new box<Point>{ v: p };
            let q = b.get();
            return q.x + q.y;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_method_named_like_array_method() {
    // A struct method `len` coexists with the array `.len` on a `[T]` field.
    let (_, _, code) = run(r#"
        struct box<T> { items: [T] }
        impl box<T> { int len(self) { return self.items.len() + 40; } }
        int main() { let b = new box<int>{ items: [1, 2] }; return b.len(); }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_method_named_copy() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        impl box<T> { box<T> copy(self) { return new box<T>{ v: self.v }; } }
        int main() { let a = new box<int>{ v: 42 }; let b = a.copy(); return b.v; }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_recursive_quicksort_with_comparator() {
    let (_, _, code) = run(r#"
        struct asc {}
        impl asc { bool lt(self, a: int, b: int) { return a < b; } }
        void qsort<T, C>(xs: [T], lo: int, hi: int, cmp: C) {
            if (lo >= hi) { return; }
            let pivot = xs[hi];
            let i = lo;
            let j = lo;
            while (j < hi) {
                if (cmp.lt(xs[j], pivot)) { let t = xs[i]; xs[i] = xs[j]; xs[j] = t; i = i + 1; }
                j = j + 1;
            }
            let t = xs[i]; xs[i] = xs[hi]; xs[hi] = t;
            qsort::<T, C>(xs, lo, i - 1, cmp);
            qsort::<T, C>(xs, i + 1, hi, cmp);
        }
        int main() {
            let xs = [5, 2, 9, 1, 7, 3, 8, 4, 6];
            qsort::<int, asc>(xs, 0, xs.len() - 1, new asc);
            let ok = true;
            let i = 1;
            while (i < xs.len()) { if (xs[i-1] > xs[i]) { ok = false; } i = i + 1; }
            if (ok && xs[0] == 1 && xs[8] == 9) { return 42; }
            return 1;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_all_array_methods_over_type_param() {
    let (_, _, code) = run(r#"
        int count_ops<T>(xs: [T]) {
            let out: [T] = [];
            out.push(xs[0]); out.push(xs[1]);
            out.insert(1, xs[2]);
            let removed = out.remove(0);
            let tail = out.slice(0, 1);
            out.extend(tail);
            let last = out.pop();
            return out.len();
        }
        int main() {
            let xs = [1, 2, 3];
            if (count_ops::<int>(xs) != 2) { return 1; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_empty_struct() {
    let (_, _, code) = run(r#"
        struct unit<T> {}
        impl unit<T> { int answer(self) { return 42; } }
        int main() { let u = new unit<int>; let v = new unit<bool>; return u.answer(); }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_functor_parameterized_by_functor() {
    let (_, _, code) = run(r#"
        struct inc {}
        impl inc { int apply(self, x: int) { return x + 1; } }
        struct twice<F> { f: F }
        impl twice<F> { int apply(self, x: int) { return self.f.apply(self.f.apply(x)); } }
        int main() { let t = new twice<inc>{ f: new inc }; return t.apply(40); }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_deep_optional_unwrap_chain() {
    let (_, _, code) = run(r#"
        struct node<T> { v: T, next: node<T>? }
        int main() {
            let c = new node<int>{ v: 3, next: none };
            let b = new node<int>{ v: 2, next: c };
            let a = new node<int>{ v: 1, next: b };
            return a.next!.next!.v * 14;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_triple_nested_return() {
    let (_, _, code) = run(r#"
        struct box<T> { v: T }
        box<box<box<T>>> triple<T>(x: T) {
            return new box<box<box<T>>>{ v: new box<box<T>>{ v: new box<T>{ v: x } } };
        }
        int main() { let b = triple::<int>(42); return b.v.v.v; }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_stack() {
    let (_, _, code) = run(r#"
        import "std/collections/stack";
        int main() {
            let s = stack.make::<int>();
            s.push(1); s.push(2); s.push(3);
            if (s.count() != 3) { return 1; }
            if (s.pop() != 3) { return 2; }
            if (s.peek() != 2) { return 3; }
            if (s.empty()) { return 4; }
            s.pop(); s.pop();
            if (!s.empty()) { return 5; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_queue() {
    let (_, _, code) = run(r#"
        import "std/collections/queue";
        int main() {
            let q = queue.make::<int>();
            let i = 0;
            while (i < 50) { q.enqueue(i); i = i + 1; }
            if (q.count() != 50) { return 1; }
            let sum = 0;
            while (!q.empty()) { sum = sum + q.dequeue(); }
            if (sum != 1225) { return 2; }
            if (q.count() != 0) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_list() {
    let (_, _, code) = run(r#"
        import "std/collections/list";
        int main() {
            let l = list.make::<int>();
            l.push_back(2); l.push_back(3); l.push_front(1);
            if (l.count() != 3) { return 1; }
            if (l.get(0) != 1) { return 2; }
            if (l.get(1) != 2) { return 3; }
            if (l.get(2) != 3) { return 4; }
            let f = l.front();
            if (f == none) { return 5; }
            if (f! != 1) { return 6; }
            if (l.pop_front() != 1) { return 7; }
            if (l.pop_front() != 2) { return 8; }
            if (l.count() != 1) { return 9; }
            l.push_back(9);
            if (l.get(1) != 9) { return 10; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_iter() {
    let (stdout, stderr, code) = run(r#"
        import "std/iter";
        import "std/io";
        import "std/conv";
        int dbl(x: int) { return x * 2; }
        bool is_even(x: int) { return x % 2 == 0; }
        bool lt4(x: int) { return x < 4; }
        int add(a: int, b: int) { return a + b; }
        [int] dup(x: int) { return [x, x]; }
        void show(x: int) { io.print(conv.int_to_string(x)); }
        int main() {
            let xs = [1, 2, 3, 4, 5, 6];
            let d = iter.map::<int, int>(xs, dbl);
            if (d.len() != 6 || d[5] != 12) { return 1; }
            let e = iter.filter::<int>(xs, is_even);
            if (e.len() != 3 || e[0] != 2 || e[2] != 6) { return 2; }
            if (iter.reduce::<int, int>(xs, 0, add) != 21) { return 3; }
            if (!iter.any::<int>(xs, is_even)) { return 4; }
            if (iter.all::<int>(xs, is_even)) { return 5; }
            if (iter.count::<int>(xs, is_even) != 3) { return 6; }
            let f = iter.find::<int>(xs, is_even);
            if (f == none || f! != 2) { return 7; }
            let p = iter.position::<int>(xs, is_even);
            if (p == none || p! != 1) { return 8; }
            if (iter.take_while::<int>(xs, lt4).len() != 3) { return 9; }
            let dw = iter.drop_while::<int>(xs, lt4);
            if (dw.len() != 3 || dw[0] != 4) { return 10; }
            let fm = iter.flat_map::<int, int>([1, 2], dup);
            if (fm.len() != 4 || fm[3] != 2) { return 11; }
            iter.each::<int>(e, show);
            return 42;
        }
    "#);
    assert_eq!(code, 42, "{stderr}");
    assert_eq!(stdout, "2\n4\n6\n", "{stderr}");
}

#[test]
fn test_std_collections_map() {
    let (_, _, code) = run(r#"
        import "std/collections/map";
        import "std/collections/hasher";
        string key(i: int) { return ['k', i as char]; }
        int main() {
            let m = map.make::<string, int, string_hasher>();
            let i = 0;
            while (i < 60) { m.set(key(i), i * 2); i = i + 1; }
            m.set(key(7), 700);
            if (m.count() != 60) { return 1; }
            if (m.remove(key(3)) == false) { return 2; }
            if (m.has(key(3))) { return 3; }
            if (m.count() != 59) { return 4; }
            let g = m.get(key(7));
            if (g == none) { return 5; }
            if (g! != 700) { return 6; }
            if (m.get(key(999)) != none) { return 7; }
            i = 0;
            let sum = 0;
            while (i < 60) {
                let v = m.get(key(i));
                if (v != none) { sum = sum + v!; }
                i = i + 1;
            }
            if (sum != 4220) { return 8; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_set() {
    let (_, _, code) = run(r#"
        import "std/collections/set";
        import "std/collections/hasher";
        int main() {
            let s = set.make::<int, int_hasher>();
            let i = 0;
            while (i < 50) { s.add(i * 2); i = i + 1; }
            if (s.count() != 50) { return 1; }
            if (s.add(4)) { return 2; }
            if (!s.has(98)) { return 3; }
            if (s.has(99)) { return 4; }
            if (!s.remove(50)) { return 5; }
            if (s.has(50)) { return 6; }
            if (s.count() != 49) { return 7; }
            i = 0;
            let c = 0;
            while (i < 100) { if (s.has(i)) { c = c + 1; } i = i + 1; }
            if (c != 49) { return 8; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_map_iteration() {
    let (_, _, code) = run(r#"
        import "std/collections/map";
        import "std/collections/hasher";
        int main() {
            let m = map.make::<int, int, int_hasher>();
            let i = 0;
            while (i < 40) { m.set(i, i * 3); i = i + 1; }
            m.remove(5);
            m.remove(20);
            let ks = m.keys();
            let vs = m.values();
            if (ks.len() != m.count()) { return 1; }
            if (vs.len() != m.count()) { return 2; }
            # keys() and values() walk in the same order
            i = 0;
            while (i < ks.len()) {
                let got = m.get(ks[i]);
                if (got == none) { return 3; }
                if (got! != vs[i]) { return 4; }
                i = i + 1;
            }
            let sum = 0;
            for k | ks { sum = sum + k; }
            if (sum != 780 - 5 - 20) { return 5; }
            sum = 0;
            for v | vs { sum = sum + v; }
            if (sum != (780 - 5 - 20) * 3) { return 6; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_collections_set_iteration() {
    let (_, _, code) = run(r#"
        import "std/collections/set";
        import "std/collections/hasher";
        int main() {
            let s = set.make::<int, int_hasher>();
            let i = 0;
            while (i < 30) { s.add(i); i = i + 1; }
            s.remove(7);
            s.remove(19);
            let xs = s.items();
            if (xs.len() != s.count()) { return 1; }
            let sum = 0;
            for x | xs { sum = sum + x; }
            if (sum != 435 - 7 - 19) { return 2; }
            for x | xs {
                if (!s.has(x)) { return 3; }
            }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_algorithm_sort_and_binary_search() {
    let (_, _, code) = run(r#"
        import "std/algorithm";
        struct asc {}
        impl asc { bool less(self, a: int, b: int) { return a < b; } }
        struct desc {}
        impl desc { bool less(self, a: int, b: int) { return a > b; } }
        int main() {
            let xs = [9, 3, 7, 1, 8, 2, 6, 0, 5, 4];
            algorithm.sort::<int, asc>(xs, new asc);
            let i = 1;
            while (i < xs.len()) { if (xs[i-1] > xs[i]) { return 1; } i = i + 1; }
            if (xs[0] != 0 || xs[9] != 9) { return 2; }
            let f = algorithm.binary_search::<int, asc>(xs, 6, new asc);
            if (f == none) { return 3; }
            if (xs[f!] != 6) { return 4; }
            if (algorithm.binary_search::<int, asc>(xs, 100, new asc) != none) { return 5; }
            algorithm.sort::<int, desc>(xs, new desc);
            if (xs[0] != 9 || xs[9] != 0) { return 6; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_std_algorithm_sorts_strings_via_comparator() {
    let (_, _, code) = run(r#"
        import "std/algorithm";
        struct str_less {}
        impl str_less {
            bool less(self, a: string, b: string) {
                let n = a.len();
                let m = b.len();
                let i = 0;
                while (i < n && i < m) {
                    if (a[i] < b[i]) { return true; }
                    if (a[i] > b[i]) { return false; }
                    i = i + 1;
                }
                return n < m;
            }
        }
        int main() {
            let xs = ["pear", "apple", "fig", "cherry", "banana"];
            algorithm.sort::<string, str_less>(xs, new str_less);
            if (xs[0] != "apple") { return 1; }
            if (xs[1] != "banana") { return 2; }
            if (xs[4] != "pear") { return 3; }
            let f = algorithm.binary_search::<string, str_less>(xs, "fig", new str_less);
            if (f == none) { return 4; }
            if (xs[f!] != "fig") { return 5; }
            if (algorithm.binary_search::<string, str_less>(xs, "kiwi", new str_less) != none) { return 6; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_non_ascii_strings_are_byte_oriented() {
    let (_, _, code) = run(r#"
        int main() {
            let s = "café";
            if (s.len() != 5) { return 1; }
            if ((s[3] as int) != 195) { return 2; }
            if ((s[4] as int) != 169) { return 3; }
            let snowman = "☃";
            if (snowman.len() != 3) { return 4; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_non_ascii_output_is_identical() {
    let (stdout, _, _) = run(r#"
        import "std/io";
        int main() { io.print("héllo wörld ☃"); return 0; }
    "#);
    assert_eq!(stdout, "héllo wörld ☃\n");
}

#[test]
fn test_first_class_function_values() {
    let (_, _, code) = run(r#"
        int add(a: int, b: int) { return a + b; }
        int sub(a: int, b: int) { return a - b; }
        int apply(f: int(int, int), x: int, y: int) { return f(x, y); }
        int twice(f: int(int), x: int) { return f(f(x)); }
        int inc(x: int) { return x + 1; }
        int main() {
            let f = add;
            if (f(3, 4) != 7) { return 1; }
            f = sub;
            if (f(10, 4) != 6) { return 2; }
            if (apply(add, 10, 4) != 14) { return 3; }
            if (twice(inc, 40) != 42) { return 4; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_function_dispatch_table() {
    let (_, _, code) = run(r#"
        int add(a: int, b: int) { return a + b; }
        int sub(a: int, b: int) { return a - b; }
        int mul(a: int, b: int) { return a * b; }
        struct handler { op: int(int, int), tag: int }
        int main() {
            let ops: [int(int, int)] = [add, sub, mul];
            if (ops[0](10, 4) != 14) { return 1; }
            if (ops[2](10, 4) != 40) { return 2; }
            ops[1] = mul;
            if (ops[1](10, 4) != 40) { return 3; }
            ops.push(sub);
            if (ops[3](10, 4) != 6) { return 4; }
            let h = new handler{ op: add, tag: 9 };
            if (h.op(20, 22) != 42) { return 5; }
            h.op = mul;
            if (h.op(6, 7) != 42) { return 6; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_function_returning_function() {
    let (_, _, code) = run(r#"
        int inc(x: int) { return x + 1; }
        int dec(x: int) { return x - 1; }
        int(int) pick(up: bool) {
            if (up) { return inc; }
            return dec;
        }
        int main() {
            let f = pick(true);
            let g = pick(false);
            if (f(41) != 42) { return 1; }
            if (g(43) != 42) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_generic_higher_order_functions() {
    let (_, _, code) = run(r#"
        [U] map<T, U>(xs: [T], f: U(T)) {
            let out: [U] = [];
            let i = 0;
            while (i < xs.len()) { out.push(f(xs[i])); i = i + 1; }
            return out;
        }
        [T] filter<T>(xs: [T], pred: bool(T)) {
            let out: [T] = [];
            let i = 0;
            while (i < xs.len()) { if (pred(xs[i])) { out.push(xs[i]); } i = i + 1; }
            return out;
        }
        A fold<T, A>(xs: [T], init: A, f: A(A, T)) {
            let acc = init;
            let i = 0;
            while (i < xs.len()) { acc = f(acc, xs[i]); i = i + 1; }
            return acc;
        }
        int dbl(x: int) { return x * 2; }
        bool is_big(x: int) { return x > 3; }
        int addi(a: int, b: int) { return a + b; }
        int main() {
            let xs = [1, 2, 3, 4, 5];
            let ys = map::<int, int>(xs, dbl);
            if (ys.len() != 5 || ys[0] != 2 || ys[4] != 10) { return 1; }
            let big = filter::<int>(xs, is_big);
            if (big.len() != 2 || big[0] != 4 || big[1] != 5) { return 2; }
            if (fold::<int, int>(xs, 0, addi) != 15) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_function_name_is_not_assignable() {
    let errors = compile_fails(&[(
        "main.kora",
        "int add(a: int, b: int) { return a + b; } int mul(a: int, b: int) { return a * b; } int main() { add = mul; return add(1, 2); }",
    )]);
    assert!(errors.contains("not assignable"), "{errors}");
}

#[test]
fn test_function_type_returns_optional() {
    // `int?(int)` binds the `?` to the return type: a function returning int?.
    let (_, _, code) = run(r#"
        int? maybe(x: int) { if (x > 0) { return x; } return none; }
        int use_it(f: int?(int), x: int) {
            let r = f(x);
            if (r == none) { return 100; }
            return r!;
        }
        int main() {
            if (use_it(maybe, 41) != 41) { return 1; }
            if (use_it(maybe, -5) != 100) { return 2; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}

#[test]
fn test_optional_function_value() {
    // `int(int)?` wraps the whole function type: an optional function.
    let (_, _, code) = run(r#"
        int inc(x: int) { return x + 1; }
        int main() {
            let f: int(int)? = inc;
            let g: int(int)? = none;
            if (f == none) { return 1; }
            if (g != none) { return 2; }
            if (f!(41) != 42) { return 3; }
            return 42;
        }
    "#);
    assert_eq!(code, 42);
}
