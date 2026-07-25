use std::collections::{HashMap, HashSet};

use crate::{
    javascript_transpiler::JavascriptTranspiler,
    lexer,
    parser::{self, ASTVisitor},
    semantic_analyzer::{Resolver, ReturnChecker, TypeChecker},
};

const PRELUDE: &str = r#"
            extern void clear();
            extern void print(a: string);
            extern string input();
            extern void sleep(ms: int);
            extern bool is_key_down(key: string);
            extern real random();
            extern void draw_clear();
            extern void set_color(c: string);
            extern void fill_rect(x: int, y: int, w: int, h: int);
            extern void fill_circle(x: int, y: int, r: int);
            extern void draw_text(s: string, x: int, y: int);
            extern void stroke_rect(x: int, y: int, w: int, h: int);
            extern void stroke_circle(x: int, y: int, r: int);
            extern void draw_line(x1: int, y1: int, x2: int, y2: int);
            extern void fill_triangle(x1: int, y1: int, x2: int, y2: int, x3: int, y3: int);
            extern void set_line_width(w: int);
            extern void set_font_size(px: int);
            extern void set_alpha(a: real);
            extern int canvas_width();
            extern int canvas_height();
            extern int text_width(s: string);
            extern int mouse_x();
            extern int mouse_y();
            extern bool is_mouse_down();
            extern void save();
            extern void restore();
            extern void translate(x: int, y: int);
            extern void rotate(a: real);
        "#;

fn transpile(source: &str) -> String {
    let full = format!("{PRELUDE}{source}");

    let tokens = lexer::Lexer::lex(&full).expect("lex");
    let module = parser::Parser::new(tokens).parse().expect("parse");
    let symbols = Resolver::new()
        .resolve(&[&module])
        .unwrap_or_else(|errs| panic!("resolve: {errs:?}"));

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    checker
        .check()
        .unwrap_or_else(|errs| panic!("type check: {errs:?}"));

    let mut return_checker = ReturnChecker::new();
    return_checker.visit_module(&module);
    return_checker
        .check()
        .unwrap_or_else(|errs| panic!("return check: {errs:?}"));

    let method_calls = super::mangled_method_calls(&symbols, &checker.method_calls);
    let async_fns =
        super::resolve_async_fns(&module, &method_calls, HashSet::from(["input".to_string()]));
    let struct_members = super::struct_member_map(&symbols);
    let mut transpiler = JavascriptTranspiler::new(
        checker.types,
        method_calls,
        checker.array_method_calls,
        struct_members,
        HashMap::new(),
        async_fns,
    );
    transpiler.visit_module(&module);
    transpiler
        .get_source()
        .map(|s| s.to_string())
        .unwrap_or_else(|errs| panic!("transpile: {errs:?}"))
}

#[test]
fn valid() {
    let source = r#"
            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                let d: char = 'a';
                if (a - b == 1) {
                    print(a, b);
                }
                if (c / 2.0 == 10.0) {
                    let d: real = c / 2.0 + 10.0;
                }
                return a;
            }

            void print(a: int, b: int) {
                while (a == 10) {
                    print(b, 1);
                    a = a - 1;
                }
            }

            int sum(a: int, b: int) {
                return a + b;
            }
        "#;

    let tokens = lexer::Lexer::lex(source).expect("lex");
    let mut parser = parser::Parser::new(tokens);
    let module = parser.parse().expect("parse");
    let symbols = Resolver::new().resolve(&[&module]).expect("resolve");

    let mut checker = TypeChecker::new(&symbols);
    checker.visit_module(&module);
    assert_eq!(checker.check().is_ok(), true);

    let method_calls = HashMap::new();
    let async_fns =
        super::resolve_async_fns(&module, &method_calls, HashSet::from(["input".to_string()]));
    let mut transpiler = JavascriptTranspiler::new(
        checker.types,
        method_calls,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        async_fns,
    );
    transpiler.visit_module(&module);
    if let Ok(source) = transpiler.get_source() {
        println!("{}", source);
    }
}

#[test]
fn test_methods_emit_mangled_global_functions() {
    let js = transpile(
        r#"
            struct P { x: int }
            impl P {
                int get(self) { return self.x; }
                P me(self) { return self; }
                void set(self, v: int) { self.x = v; }
            }
            int main() {
                let p = new P;
                p.set(3);
                return p.me().get();
            }
        "#,
    );
    assert!(js.contains("function P$get(self)"), "{js}");
    assert!(js.contains("function P$me(self)"), "{js}");
    assert!(js.contains("function P$set(self, v)"), "{js}");
    assert!(js.contains("P$set(p,3)"), "{js}");
    assert!(js.contains("P$get(P$me(p))"), "{js}");
}

#[test]
fn test_async_coloring_propagates_through_method_calls() {
    let js = transpile(
        r#"
            struct P { x: int }
            impl P {
                string ask(self) { return input(); }
                string relay(self) { return self.ask(); }
            }
            int main() {
                let p = new P;
                let a = p.relay();
                return 0;
            }
        "#,
    );
    assert!(js.contains("async function P$ask(self)"), "{js}");
    assert!(js.contains("async function P$relay(self)"), "{js}");
    assert!(js.contains("async function main()"), "{js}");
    assert!(js.contains("(await P$ask(self))"), "{js}");
    assert!(js.contains("(await P$relay(p))"), "{js}");
}

#[test]
fn test_array_methods_emit_js_builtins() {
    let js = transpile(
        r#"
            int main() {
                let a = [1, 2];
                a.push(3);
                a.insert(0, 4);
                let x = a.remove(1);
                let y = a.pop();
                let b = a.slice(0, 1);
                a.extend([9, 9]);
                return a.len() + x + y + b.len();
            }
        "#,
    );
    assert!(js.contains("a.push(3)"), "{js}");
    assert!(js.contains("__kora_runtime_insert(a,0,4)"), "{js}");
    assert!(js.contains("__kora_runtime_remove(a,1)"), "{js}");
    assert!(js.contains("__kora_runtime_pop(a)"), "{js}");
    assert!(js.contains("a.slice(0,1)"), "{js}");
    assert!(js.contains("a.push(..."), "{js}");
    assert!(js.contains("a.length"), "{js}");
}

#[test]
fn test_copy_and_array_plus_emit() {
    let js = transpile(
        r#"
            struct P { x: int }
            int main() {
                let a = [1, 2];
                let b = copy(a);
                let c = a + b;
                let p = new P { x: 1 };
                let q = copy(p);
                return c.len() + q.x;
            }
        "#,
    );
    assert!(js.contains("Array.from(a)"), "{js}");
    assert!(js.contains("({...p})") || js.contains("{...p}"), "{js}");
    assert!(js.contains("a.concat(b)"), "{js}");
    assert!(js.contains("({x:1})") || js.contains("{x:"), "{js}");
}

#[test]
fn test_array_equality_emits_structural_compare() {
    let js = transpile(
        r#"
            int main() {
                let s = "abc";
                if (s == "quit") { return 1; }
                if (s != "exit") { return 2; }
                if (s[0] == 'a') { return 3; }
                return 0;
            }
        "#,
    );
    assert!(js.contains("__kora_runtime_equality_intrinsic("), "{js}");
    assert!(js.contains("!__kora_runtime_equality_intrinsic("), "{js}");
    assert!(js.contains("__kora_runtime_index(s,0)==='a'"), "{js}");
    assert!(
        !js.contains("__kora_runtime_equality_intrinsic(__kora_runtime_index(s,0)"),
        "{js}"
    );
}

#[test]
fn test_runtime_checks_emit_intrinsics() {
    let js = transpile(
        r#"
            int main() {
                let a = 7 / 2;
                let b = 7 % 2;
                let c = 7.0 / 2.0;
                return a + b;
            }
        "#,
    );
    assert!(js.contains("__kora_runtime_div(7,2)"), "{js}");
    assert!(js.contains("__kora_runtime_mod(7,2)"), "{js}");
    // real division stays raw `/` (IEEE Infinity on /0, both backends)
    assert!(js.contains("let c = 7/2;"), "{js}");
}

#[test]
fn test_array_indexing_is_bounds_checked() {
    let js = transpile(
        r#"
            int main() {
                let a = [1, 2, 3];
                a[1] = a[0];
                return a[2];
            }
        "#,
    );
    assert!(
        js.contains("__kora_runtime_index_set(a,1,__kora_runtime_index(a,0))"),
        "{js}"
    );
    assert!(js.contains("return __kora_runtime_index(a,2);"), "{js}");
}

#[test]
fn test_cast_semantics_match_native_codegen() {
    let js = transpile(
        r#"
            int main() {
                let a = 2.9 as int;
                let b = 'a' as int;
                let c = 65 as char;
                let d = 1 as real;
                let e = 2.9 as char;
                return a + b;
            }
        "#,
    );
    assert!(js.contains("Math.trunc(2.9)"), "{js}");
    assert!(js.contains(".charCodeAt(0)"), "{js}");
    assert!(js.contains("String.fromCharCode(65)"), "{js}");
    assert!(js.contains("String.fromCharCode(Math.trunc(2.9))"), "{js}");
}

#[test]
fn test_string_literals_are_mutable_arrays() {
    let js = transpile(
        r#"
            int main() {
                let s = "abc";
                s[0] = 'x';
                return s.len();
            }
        "#,
    );
    assert!(js.contains(r#"Array.from("abc")"#), "{js}");
}

#[test]
fn test_scalar_arrays_are_zero_filled() {
    let js = transpile(
        r#"
            int main() {
                let a = new int[3];
                let b = new real[3];
                let c = new bool[3];
                let d = new char[3];
                return a[0];
            }
        "#,
    );
    assert!(js.contains(".fill(0)"), "{js}");
    assert!(js.contains(".fill(0.0)"), "{js}");
    assert!(js.contains(".fill(false)"), "{js}");
    assert!(js.contains(".fill(\"\\0\")"), "{js}");
}

#[test]
fn test_optionals_emit() {
    let js = transpile(
        r#"
            struct Node { value: int, next: Node? }
            int main() {
                let x: int? = 5;
                let y: int? = none;
                let z = x!;
                if (y == none) { return 1; }
                let n = new Node { value: 1, next: none };
                if (n.next != none) { return n.next!.value; }
                return z;
            }
        "#,
    );
    assert!(js.contains("let y = null"), "{js}");
    assert!(js.contains("__kora_runtime_unwrap(x)"), "{js}");
    assert!(js.contains("__kora_runtime_unwrap(n.next).value"), "{js}");
    assert!(js.contains("y==null"), "{js}");
    assert!(js.contains("n.next!=null"), "{js}");
    assert!(js.contains("function __kora_runtime_unwrap("), "{js}");
}

#[test]
fn test_bare_new_struct_is_zero_filled() {
    let js = transpile(
        r#"
            struct Node { value: int, flag: bool, tags: [int], next: Node? }
            int main() {
                let n = new Node;
                let a = new Node[2];
                return n.value + a.len();
            }
        "#,
    );
    assert!(
        js.contains("({value:0,flag:false,tags:[],next:null})"),
        "{js}"
    );
    assert!(
        js.contains("Array.from({length:__kora_runtime_check_len(2)},()=>({value:0,flag:false,tags:[],next:null}))"),
        "{js}"
    );
}

#[test]
fn test_bare_new_struct_zero_fill_is_recursive() {
    let js = transpile(
        r#"
            struct Point { x: int, y: int }
            struct Line { a: Point, b: Point }
            int main() {
                let l = new Line;
                let g = new Line[2];
                return l.a.x + g.len();
            }
        "#,
    );
    // A default-constructible struct member is "zero".
    assert!(js.contains("({a:({x:0,y:0}),b:({x:0,y:0})})"), "{js}");
    assert!(
        js.contains(
            "Array.from({length:__kora_runtime_check_len(2)},()=>({a:({x:0,y:0}),b:({x:0,y:0})}))"
        ),
        "{js}"
    );
}

fn transpile_program(
    entry: &str,
    files: Vec<(&str, String)>,
    async_externs: HashSet<String>,
) -> String {
    use std::path::Path;
    let map: HashMap<String, String> = files.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let compiled = crate::compile(entry, |p: &Path| {
        p.to_str().and_then(|s| map.get(s)).cloned()
    })
    .unwrap_or_else(|e| panic!("compile: {e:?}"));

    let method_calls = super::mangled_method_calls(&compiled.symbols, &compiled.method_calls);
    let function_names = super::function_names(&compiled.symbols, &compiled.program);
    let struct_members = super::struct_member_map(&compiled.symbols);
    let async_fns = super::resolve_async_fns(
        &compiled.program.modules[0].module,
        &method_calls,
        async_externs,
    );
    let modules: Vec<&parser::Module> =
        compiled.program.modules.iter().map(|m| &m.module).collect();
    let mut transpiler = JavascriptTranspiler::new(
        compiled.types,
        method_calls,
        compiled.array_method_calls,
        struct_members,
        function_names,
        async_fns,
    );
    transpiler.emit_program(&modules);
    transpiler
        .get_source()
        .map(|s| s.to_string())
        .unwrap_or_else(|e| panic!("transpile: {e:?}"))
}

#[test]
fn test_imported_stdlib_functions_are_mangled_and_called() {
    let js = transpile_program(
        "main.kora",
        vec![(
            "main.kora",
            r#"import "std/conv";
               int main() { return conv.int_to_string(42).len(); }"#
                .to_string(),
        )],
        HashSet::new(),
    );
    assert!(js.contains("function std$conv$int_to_string("), "{js}");
    assert!(js.contains("std$conv$int_to_string(42)"), "{js}");
    assert!(js.contains("function main("), "{js}");
    assert!(!js.contains("conv.int_to_string"), "{js}");
}

#[test]
fn test_str_module_transpiles_and_cross_calls_within_module() {
    let js = transpile_program(
        "main.kora",
        vec![(
            "main.kora",
            r#"import "std/str";
               int main() { if (str.contains("ab", "b")) { return 1; } return 0; }"#
                .to_string(),
        )],
        HashSet::new(),
    );
    assert!(js.contains("function std$str$contains("), "{js}");
    assert!(js.contains("function std$str$index_of("), "{js}");
    assert!(js.contains("std$str$index_of(haystack,needle)"), "{js}");
    assert!(js.contains("std$str$contains("), "{js}");
}

#[test]
fn transpiles_ui_examples() {
    let examples: &[(&str, &str)] = &[
        ("mandelbrot", include_str!("../../res/mandelbrot.kora")),
        ("sudoku", include_str!("../../res/sudoku.kora")),
        ("snake", include_str!("../../res/snake.kora")),
        ("tetris", include_str!("../../res/tetris.kora")),
        ("pong", include_str!("../../res/pong.kora")),
        ("doom", include_str!("../../res/doom.kora")),
        ("pacman", include_str!("../../res/pacman.kora")),
    ];

    let async_externs = HashSet::from(["input".to_string(), "sleep".to_string()]);
    for (name, source) in examples {
        println!("transpiling {name}.kora");
        transpile_program(
            "main.kora",
            vec![("main.kora", format!("{PRELUDE}{source}"))],
            async_externs.clone(),
        );
    }
}
