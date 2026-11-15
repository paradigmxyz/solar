//! Libraries that must fail before the loader calls a typed Rust entry point.

#[cfg(not(missing_abi))]
#[unsafe(no_mangle)]
pub static solar_rust_pass_plugin_abi_v1: [u8; 32] = if cfg!(wrong_abi) {
    [0; 32]
} else {
    *include_bytes!(env!("SOLAR_TEST_ABI_FILE"))
};

#[unsafe(no_mangle)]
pub fn not_a_plugin() {}

// A mismatched fingerprint must be rejected before this incompatible entry point is called.
#[cfg(wrong_abi)]
#[unsafe(no_mangle)]
pub fn solar_rust_pass_plugin_v1() -> ! { std::process::abort() }
