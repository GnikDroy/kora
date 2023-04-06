use std::collections::HashSet;

use super::symbol_table::*;
use crate::parser::*;

#[derive(Default)]
pub struct UnidentifiedIdentifierChecker {
    global_symbols: SymbolTable,
    current_symbols: SymbolTable,
    unidentified_identifiers: HashSet<String>,
}

impl UnidentifiedIdentifierChecker {
    pub fn new(mut symbols: SymbolTable) -> UnidentifiedIdentifierChecker {
        symbols.reverse();
        UnidentifiedIdentifierChecker {
            global_symbols: symbols,
            ..Default::default()
        }
    }

    pub fn check(&self) -> Result<(), &HashSet<String>> {
        if self.unidentified_identifiers.is_empty() {
            Ok(())
        } else {
            Err(&self.unidentified_identifiers)
        }
    }
}

impl ASTVisitor for UnidentifiedIdentifierChecker {
    fn visit_identifier(&mut self, name: &String) {
        if self.current_symbols.resolve(name).is_none() {
            self.unidentified_identifiers.insert(name.clone());
        }
    }

    fn visit_enter_scope(&mut self) {
        // the stack structure is same, so unwrap() is guaranteed to work.
        let scope = self.global_symbols.pop_scope().unwrap();
        self.current_symbols.add_scope(scope);
    }

    fn visit_exit_scope(&mut self) {
        self.current_symbols.pop_scope();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer,
        parser::{self, ASTVisitor},
        semantic_analyzer::symbol_table::SymbolTable,
    };

    use super::UnidentifiedIdentifierChecker;

    #[test]
    fn valid() {
        let source = r#"
            extern nil print(b: [char], a: int);

            int main() {
                let a: int = 5;
                let b: int = 6;
                let c: real = 6.2345;
                if (a - b) {
                    print("Hello World", 5);
                }
                print("Oh no!", 5);
                ret a;
            }

            int sum(a: int, b: int) {
                ret a + b;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let mut symbol_table = SymbolTable::new();
        symbol_table.visit_module(&module);

        let mut checker = UnidentifiedIdentifierChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_ok(),
            true,
            "source_text: {}, symbol_table: {:#?} unidentified: {:?}",
            source,
            symbol_table,
            checker.check().unwrap_err()
        );
    }

    #[test]
    fn invalid() {
        let source = r#"
            extern nil print(b: [char], a: int);

            int main() {
                let a: int = 5 as int;
                let b: int = 6;
                let c: real = 6.2345 + unident_1;
                unident_2;
                if (a - b) {
                    unident_3;
                    print("Hello World", 5);
                }
                unident_4;
                print("Oh no!", 5);
                ret a;
            }

            int sum(a: int, b: int) {
                unident_5;
                ret a + b;
            }
        "#;

        let tokens = lexer::Lexer::lex(source).expect("lex");
        let mut parser = parser::Parser::new(tokens);
        let module = parser.parse().expect("parse");
        let mut symbol_table = SymbolTable::new();
        symbol_table.visit_module(&module);

        let mut checker = UnidentifiedIdentifierChecker::new(symbol_table.clone());
        checker.visit_module(&module);
        assert_eq!(
            checker.check().is_err() && checker.check().unwrap_err().len() == 5,
            true,
            "source_text: {}, symbol_table: {:#?} unidentified: {:?}",
            source,
            symbol_table,
            checker.check().unwrap_err()
        );
    }
}
