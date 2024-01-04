//! The main entry point for the Sulk compiler.

use std::{path::Path, process::ExitCode};
use sulk_interface::SourceMap;
use sulk_parse::{Lexer, ParseSess, Parser};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: {} <path>", std::env::args().next().unwrap());
        return ExitCode::FAILURE;
    };
    let path = Path::new(&path);
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
