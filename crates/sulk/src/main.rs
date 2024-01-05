//! The main entry point for the Sulk compiler.

use clap::Parser as _;
use std::process::ExitCode;
use sulk_interface::SourceMap;
use sulk_parse::{Lexer, ParseSess, Parser};

pub mod cli;

fn main() -> ExitCode {
    let opts = cli::Opts::parse();
    if opts.paths.len() != 1 {
        eprintln!("multiple files are not yet supported");
        return ExitCode::FAILURE;
    }
    let path = &opts.paths[0];
    if !path.exists() {
        eprintln!("file {} does not exist", path.display());
        return ExitCode::FAILURE;
    }

    sulk_interface::enter_with_exit_code(|| {
        let source_map = SourceMap::empty();
        let file = source_map.load_file(path).unwrap();
        let sess = ParseSess::with_tty_emitter(source_map.into());

        let tokens = Lexer::new(&sess.dcx, file.src.as_deref().unwrap()).into_tokens();
        sess.dcx.has_errors()?;
        // eprintln!("tokens: {tokens:#?}");

        let mut parser = Parser::new(&sess, tokens);
        let file = parser.parse_file().map_err(|e| e.emit())?;
        sess.dcx.has_errors()?;
        let _ = file;
        // eprintln!("file: {file:#?}");

        Ok(())
    })
}
