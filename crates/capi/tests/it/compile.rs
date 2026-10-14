use solar_capi::{solidity_compile, solidity_free};
use std::{
    ffi::CStr,
    ptr,
    time::{Duration, Instant},
};

const INPUT: &CStr = cr#"{
  "language": "Solidity",
  "sources": { "A.sol": { "content": "contract A { function f() public pure returns (uint) { return 1; } }" } },
  "settings": { "outputSelection": { "*": { "*": ["abi", "evm.bytecode.object"] } } }
}"#;

fn compile() -> String {
    unsafe {
        let output = solidity_compile(INPUT.as_ptr(), None, ptr::null_mut());
        let s = CStr::from_ptr(output).to_str().unwrap().to_owned();
        solidity_free(output);
        s
    }
}

/// Each call must free its thread pool instead of leaking one per call.
#[test]
fn compile_frees_thread_pool() {
    let threads = || std::fs::read_dir("/proc/self/task").unwrap().count();
    let before = threads();
    let output = compile();
    assert!(output.contains("\"bytecode\""), "{output}");
    for _ in 0..10 {
        compile();
    }
    // Pool threads exit asynchronously after the pool drops.
    let deadline = Instant::now() + Duration::from_secs(30);
    while threads() > before && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(threads(), before);
}
