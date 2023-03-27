use colored::Colorize;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct LexerError<'a> {
    pub msg: &'a str,
    pub col: usize,
    pub row: usize,
    pub line: String,
    pub suggestion: String,
}

impl<'a> Error for LexerError<'a> {}

impl<'a> fmt::Display for LexerError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.line.len() < 60 {}
        let error_line = format!("{}: {}\n", "error".red().bold(), self.msg.white().bold());
        let src_line = format!(
            "{} {}:{}\n",
            "-->".blue().bold(),
            self.col.to_string().bold(),
            self.row.to_string().bold()
        );
        let len = self.col.to_string().len();
        let display_line = format!(
            "{: >width$} {}\n{: >width$} {} {}\n{: >width$} {}{: >col$}{}\n",
            "",
            "|".blue().bold(),
            self.col.to_string().bold().blue(),
            "|".blue().bold(),
            self.line,
            "",
            "|".blue().bold(),
            "",
            "^".yellow().bold(),
            width = len,
            col = self.row
        );
        let suggestion_line = format!(
            "{: >width$} {} {}",
            "",
            "=".blue().bold(),
            self.suggestion.yellow().bold(),
            width = len
        );
        write!(
            f,
            "{}{}{}{}",
            error_line, src_line, display_line, suggestion_line
        )
    }
}
