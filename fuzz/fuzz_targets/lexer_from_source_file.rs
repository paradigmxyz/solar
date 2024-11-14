#![no_main]

use libfuzzer_sys::fuzz_target;

use solar_interface::{diagnostics::DiagCtxt, Session};
use solar_parse::Lexer;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = String::from_utf8(data.to_vec()) {
        let sess = Session::empty(DiagCtxt::new_early());
        let _ = Lexer::new(&sess, &s);
    }
});
