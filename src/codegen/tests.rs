use super::lower;
use inkwell::OptimizationLevel;
use inkwell::context::Context;
use std::path::Path;

fn run_main(source: &str) -> i64 {
    run_main_files(&[("main.kora", source)])
}

fn run_main_files(files: &[(&str, &str)]) -> i64 {
    let program = crate::compile(files[0].0, |path: &Path| {
        files
            .iter()
            .find(|(name, _)| path == Path::new(name))
            .map(|(_, source)| source.to_string())
    })
    .expect("front-end");

    let context = Context::create();
    let llvm = lower(&context, &program).expect("codegen");
    llvm.verify()
        .unwrap_or_else(|e| panic!("invalid IR:\n{}\n{}", llvm.print_to_string(), e));

    let engine = llvm
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("jit");

    extern "C" fn jit_panic(_message: *const i8) {
        panic!("__kora_panic reached in a JIT test");
    }
    extern "C" fn jit_handle_a() -> *const u8 {
        0xA0 as *const u8
    }
    extern "C" fn jit_handle_b() -> *const u8 {
        0xB0 as *const u8
    }

    extern "C" fn jit_ret_neg_i32() -> i32 {
        -7
    }
    extern "C" fn jit_ret_max_u32() -> u32 {
        u32::MAX
    }
    extern "C" fn jit_echo_i32(x: i32) -> i32 {
        x
    }
    extern "C" fn jit_ret_half_f32() -> f32 {
        0.5
    }
    extern "C" fn jit_echo_i8(x: i8) -> i8 {
        x
    }
    extern "C" fn jit_echo_u8(x: u8) -> u8 {
        x
    }
    extern "C" fn jit_echo_i16(x: i16) -> i16 {
        x
    }
    extern "C" fn jit_echo_u16(x: u16) -> u16 {
        x
    }
    extern "C" fn jit_flip_bool(x: bool) -> bool {
        !x
    }

    let stand_ins = [
        ("__kora_panic", jit_panic as *const ()),
        ("make_handle_a", jit_handle_a as *const ()),
        ("make_handle_b", jit_handle_b as *const ()),
        ("ret_neg_i32", jit_ret_neg_i32 as *const ()),
        ("ret_max_u32", jit_ret_max_u32 as *const ()),
        ("echo_i32", jit_echo_i32 as *const ()),
        ("ret_half_f32", jit_ret_half_f32 as *const ()),
        ("echo_i8", jit_echo_i8 as *const ()),
        ("echo_u8", jit_echo_u8 as *const ()),
        ("echo_i16", jit_echo_i16 as *const ()),
        ("echo_u16", jit_echo_u16 as *const ()),
        ("flip", jit_flip_bool as *const ()),
        ("kora", jit_echo_i32 as *const ()),
    ];
    for (name, addr) in stand_ins {
        if let Some(f) = llvm.get_function(name) {
            engine.add_global_mapping(&f, addr as usize);
        }
    }

    unsafe {
        engine
            .get_function::<unsafe extern "C" fn() -> i64>("__kora_main")
            .expect("__kora_main")
            .call()
    }
}

#[test]
fn test_arithmetic() {
    assert_eq!(run_main("int main() { return 2 + 3 * 4 - 10 / 2; }"), 9);
    assert_eq!(run_main("int main() { return 17 % 5; }"), 2);
    assert_eq!(run_main("int main() { return -(3 - 5); }"), 2);
}

#[test]
fn test_real_arithmetic() {
    assert_eq!(
        run_main("int main() { return (1.5 * 4.0 + 1.0 / 2.0) as int; }"),
        6
    );
}

#[test]
fn test_variables_and_assignment() {
    assert_eq!(
        run_main("int main() { let a: int = 3; a = a + 4; return a; }"),
        7
    );
    assert_eq!(
        run_main("int main() { let a: int = 0; let b: int = 0; a = b = 5; return a + b; }"),
        10
    );
}

#[test]
fn test_shadowing() {
    let source = r#"
        int main() {
            let x: int = 1;
            if (true) {
                let x: int = 100;
                x = x + 1;
            }
            return x;
        }
    "#;
    assert_eq!(run_main(source), 1);
}

#[test]
fn test_if_else() {
    let source = r#"
        int max(a: int, b: int) {
            if (a > b) { return a; } else { return b; }
        }
        int main() { return max(3, 11) + max(7, 2); }
    "#;
    assert_eq!(run_main(source), 18);
}

#[test]
fn test_while_loop() {
    let source = r#"
        int main() {
            let a: int = 0;
            let b: int = 1;
            let i: int = 0;
            while (i < 10) {
                let next: int = a + b;
                a = b;
                b = next;
                i = i + 1;
            }
            return a;
        }
    "#;
    assert_eq!(run_main(source), 55);
}

#[test]
fn test_recursion() {
    let source = r#"
        int fib(n: int) {
            if (n < 2) { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        int main() { return fib(10); }
    "#;
    assert_eq!(run_main(source), 55);
}

#[test]
fn test_for_break_continue() {
    let source = r#"
        int main() {
            let sum = 0;
            for (let i = 0; i < 100; i = i + 1) {
                if (i % 2 == 0) { continue; }
                if (i > 9) { break; }
                sum = sum + i;
            }
            return sum;
        }
    "#;
    assert_eq!(run_main(source), 25);
}

#[test]
fn test_bitwise() {
    assert_eq!(run_main("int main() { return 12 & 10; }"), 8);
    assert_eq!(run_main("int main() { return 12 | 10; }"), 14);
    assert_eq!(run_main("int main() { return 12 ^ 10; }"), 6);
    assert_eq!(run_main("int main() { return 3 << 4; }"), 48);
    assert_eq!(run_main("int main() { return -16 >> 2; }"), -4);
}

#[test]
fn test_casts() {
    assert_eq!(run_main("int main() { return 2.9 as int; }"), 2);
    assert_eq!(run_main("int main() { return 'a' as int; }"), 97);
    assert_eq!(
        run_main("int main() { return (('a' as int + 1) as char) as int; }"),
        98
    );
    assert_eq!(run_main("int main() { return (1 as real) as int; }"), 1);
}

#[test]
fn test_bool_logic() {
    let source = r#"
        int main() {
            let t: bool = 1 < 2 && 3 != 4;
            let f: bool = t && false;
            if (t || f) {
                if (!f) { return 1; }
            }
            return 0;
        }
    "#;
    assert_eq!(run_main(source), 1);
}

#[test]
fn test_short_circuit_skips_rhs() {
    // The rhs recursion would overflow the stack if && doesn't short-circuit.
    let source = r#"
        bool diverge() { return diverge(); }
        int main() {
            if (false && diverge()) { return 1; }
            if (true || diverge()) { return 2; }
            return 3;
        }
    "#;
    assert_eq!(run_main(source), 2);
}

#[test]
fn test_char_comparisons() {
    let source = r#"
        int main() {
            if ('a' < 'b' && 'z' > 'y' && 'c' == 'c') { return 1; }
            return 0;
        }
    "#;
    assert_eq!(run_main(source), 1);
}

#[test]
fn test_void_function_call() {
    let source = r#"
        void nop() { }
        void maybe_return(a: bool) {
            if (a) { return; }
        }
        int main() { nop(); maybe_return(true); return 7; }
    "#;
    assert_eq!(run_main(source), 7);
}

#[test]
fn test_let_inference() {
    let source = r#"
        int main() {
            let a = 3;
            let b = a * 4;
            let r = 1.5 + 2.5;
            let c = 'a';
            let big = b > a && r == 4.0;
            if (big) { return b + (r as int) + (c as int); }
            return 0;
        }
    "#;
    assert_eq!(run_main(source), 12 + 4 + 97);
}

#[test]
fn test_dead_code_after_return() {
    let source = r#"
        int main() {
            while (true) {
                return 4;
            }
            return 5;
        }
    "#;
    assert_eq!(run_main(source), 4);
}

#[test]
fn test_cross_module_calls() {
    let result = run_main_files(&[
        (
            "main.kora",
            r#"
                import "lib.kora";
                int main() { return lib.triple(add(3, 4)); }
                int add(a: int, b: int) { return a + b; }
            "#,
        ),
        ("lib.kora", "int triple(x: int) { return x * 3; }"),
    ]);
    assert_eq!(result, 21);
}

#[test]
fn test_opaque_handles() {
    let result = run_main(
        r#"
            extern opaque make_handle_a();
            extern opaque make_handle_b();
            opaque pick(h: opaque) { return h; }
            int main() {
                let a = make_handle_a();
                let b = make_handle_b();
                let r = 0;
                if (a == make_handle_a()) { r = r + 1; }
                if (a != b) { r = r + 2; }
                if (pick(a) == a) { r = r + 4; }
                let m: opaque? = none;
                if (m == none) { r = r + 8; }
                m = a;
                if (m != none && m! == a) { r = r + 16; }
                if (m == pick(a)) { r = r + 32; }
                return r;
            }
        "#,
    );
    assert_eq!(result, 63);
}

#[test]
fn test_extern_scalar_marshalling() {
    let result = run_main(
        r#"
            extern int32 ret_neg_i32();
            extern uint32 ret_max_u32();
            extern int32 echo_i32(x: int32);
            extern float32 ret_half_f32();
            int main() {
                let r = 0;
                if (ret_neg_i32() == -7) { r = r + 1; }
                if (ret_max_u32() == 4294967295) { r = r + 2; }
                if (echo_i32(4294967296 + 5) == 5) { r = r + 4; }
                if (ret_half_f32() == 0.5) { r = r + 8; }
                return r;
            }
        "#,
    );
    assert_eq!(result, 15);
}

#[test]
fn test_extern_narrow_int_marshalling() {
    let result = run_main(
        r#"
            extern int8 echo_i8(x: int8);
            extern uint8 echo_u8(x: uint8);
            extern int16 echo_i16(x: int16);
            extern uint16 echo_u16(x: uint16);
            int main() {
                let r = 0;
                if (echo_i8(300) == 44) { r = r + 1; }
                if (echo_i8(-1) == -1) { r = r + 2; }
                if (echo_u8(-1) == 255) { r = r + 4; }
                if (echo_i16(65543) == 7) { r = r + 8; }
                if (echo_i16(-2) == -2) { r = r + 16; }
                if (echo_u16(-1) == 65535) { r = r + 32; }
                return r;
            }
        "#,
    );
    assert_eq!(result, 63);
}

#[test]
fn test_extern_bool_marshalling() {
    let result = run_main(
        r#"
            extern bool flip(b: bool);
            int main() {
                let r = 0;
                if (flip(false)) { r = r + 1; }
                if (!flip(true)) { r = r + 2; }
                if (flip(1 == 2)) { r = r + 4; }
                return r;
            }
        "#,
    );
    assert_eq!(result, 7);
}

#[test]
fn test_thunk_names_cannot_collide_with_mangled_functions() {
    let result = run_main(
        r#"
            extern int32 kora(x: int32);
            int thunk() { return 2; }
            int main() { return kora(40) + thunk(); }
        "#,
    );
    assert_eq!(result, 42);
}
