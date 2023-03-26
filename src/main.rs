#![feature(exclusive_range_pattern)]
#![feature(let_chains)]

mod lexer;

fn main() {
    let source = std::fs::read_to_string("res/1.k").unwrap();
    match lexer::lex(source) {
        Ok(_) => println!("Lexer successful!"),
        Err(e) => println!("{}", e),
    }
}
