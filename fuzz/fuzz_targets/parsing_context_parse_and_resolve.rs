#![no_main]

use std::io::Write;

use libfuzzer_sys::fuzz_target;

use solar_interface::Session;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    let mut file = NamedTempFile::new().expect("Failed to create named temporary file");
    file.write_all(data).expect("Failed to write to temporary file");

    let path = file.into_temp_path();
    let path = path.keep().expect("Failed to persist temporary file");

    let sess = Session::builder().with_buffer_emitter(solar_interface::ColorChoice::Auto).build();

    let _ = sess.enter(|| -> solar_interface::Result<()> {
        let mut pcx = solar_sema::ParsingContext::new(&sess);
        pcx.load_file(&path).unwrap();
        pcx.parse_and_resolve()
    });
});
