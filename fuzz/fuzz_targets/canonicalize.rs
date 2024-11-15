#![no_main]

use libfuzzer_sys::fuzz_target;

use solar_interface::canonicalize;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = String::from_utf8(data.to_vec()) {
        let _ = canonicalize(s);
    }
});
