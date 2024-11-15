#![no_main]

use std::io::Write;

use libfuzzer_sys::fuzz_target;

use tempfile::NamedTempFile;

use solar_cli::{parse_args, run_compiler_args};

fuzz_target!(|data: &[u8]| {
    let mut file = NamedTempFile::new().expect("Failed to create named temporary file");
    file.write_all(data).expect("Failed to write to temporary file");

    let path = file.into_temp_path();
    let path = path.keep().expect("Failed to persist temporary file");

    let args = parse_args(["solar", path.to_str().unwrap()]).expect("Invalid CLI arguments");

    let _ = run_compiler_args(args);
});