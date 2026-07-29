use std::process::ExitCode;

use clap::Parser;

mod cli;

fn main() -> ExitCode {
    let args = cli::Args::parse();
    match cli::run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprint!("{}", e);
            if !e.ends_with('\n') {
                eprintln!();
            }
            ExitCode::FAILURE
        }
    }
}
