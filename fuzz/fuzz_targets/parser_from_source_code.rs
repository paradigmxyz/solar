#![no_main]

use libfuzzer_sys::fuzz_target;

use solar_interface::{diagnostics::DiagCtxt, source_map::FileName, Session};
use solar_parse::{ast::Arena, Parser};

const FILENAME: &str = "cargo-fuzz";

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = String::from_utf8(data.to_vec()) {
        let sess = Session::empty(DiagCtxt::new_early());
        let arena = Arena::new();
        let _ = Parser::from_source_code(&sess, &arena, FileName::Custom(FILENAME.to_string()), s);
    }
});
