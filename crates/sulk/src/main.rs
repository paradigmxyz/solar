//! The main entry point for the Sulk compiler.

use clap::Parser as _;
use std::process::ExitCode;
use sulk_data_structures::sync::Lrc;
use sulk_interface::SourceMap;
use sulk_parse::{Lexer, ParseSess, Parser};

pub mod cli;

fn main() -> ExitCode {
    let opts = cli::Opts::parse();
    for path in &opts.paths {
        let Ok(meta) = path.metadata() else {
            eprintln!("{} does not exist", path.display());
            return ExitCode::FAILURE;
        };
        if !meta.is_file() {
            eprintln!("{} is not a file", path.display());
            return ExitCode::FAILURE;
        }
    }

    sulk_interface::enter_with_exit_code(|| {
        let source_map = SourceMap::empty();
        for path in &opts.paths {
            let _file = source_map.load_file(path).unwrap();
        }
        let source_map = Lrc::new(source_map);

        let mut result = Ok(());
        for file in source_map.files().iter() {
            let sess = ParseSess::with_tty_emitter(source_map.clone());
            let tokens = Lexer::from_source_file(&sess, file).into_tokens();
            if let Err(e) = sess.dcx.has_errors() {
                result = Err(e);
                continue;
            }

            let mut parser = Parser::new(&sess, tokens);
            let file = parser.parse_file().map_err(|e| e.emit())?;
            let _ = file;
            // eprintln!("file: {file:#?}");
        }
        result
    })
}
