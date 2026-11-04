//! Deliberately invalid native plugins, built by the integration tests without dependencies.

use std::ffi::c_void;
type Writer = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> u32;

#[cfg(not(missing))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn solar_pass_plugin_v1(input: *const u8, length: usize, write: Writer, context: *mut c_void) -> u32 {
    let input = unsafe { std::slice::from_raw_parts(input, length) };
    let describe = input.windows(8).any(|bytes| bytes == b"describe");
    let response = if describe {
        if cfg!(bad_version) {
            r#"{"api_version":999,"name":"broken","version":"1","passes":[]}"#
        } else if cfg!(wrong_type) || cfg!(invalid_function) {
            r#"{"api_version":1,"name":"broken","version":"1","passes":[{"name":"fail","ir":"mir","phase":"semantic","enabled_by_default":true}]}"#
        } else {
            r#"{"api_version":1,"name":"broken","version":"1","passes":[{"name":"fail","ir":"evm-ir","phase":null,"enabled_by_default":true}]}"#
        }
    } else if cfg!(wrong_type) {
        r#"{"edits":[{"edit":"replace_mir_value","function":0,"value":0,"replacement":1}]}"#
    } else if cfg!(invalid_function) {
        r#"{"edits":[{"edit":"replace_mir_value","function":4294967295,"value":0,"replacement":1}]}"#
    } else if cfg!(invalid_block) {
        r#"{"edits":[{"edit":"splice_evm_instructions","block":4294967295,"start":0,"remove":0,"instructions":[]}]}"#
    } else if cfg!(invalid_ir) {
        r#"{"edits":[{"edit":"splice_evm_instructions","block":0,"start":0,"remove":0,"instructions":[{"kind":"opcode","opcode":80,"source":null}]}]}"#
    } else if cfg!(wrong_fork) {
        r#"{"edits":[{"edit":"splice_evm_instructions","block":0,"start":0,"remove":0,"instructions":[{"kind":"push","value":"0x0","source":null},{"kind":"opcode","opcode":92,"source":null},{"kind":"opcode","opcode":80,"source":null}]}]}"#
    } else if cfg!(invalid_opcode) {
        r#"{"edits":[{"edit":"splice_evm_instructions","block":0,"start":0,"remove":0,"instructions":[{"kind":"opcode","opcode":230,"source":null}]}]}"#
    } else if cfg!(malformed) {
        "not JSON"
    } else {
        "intentional failure"
    };
    if cfg!(no_response) { return 0; }
    unsafe { write(context, response.as_ptr(), response.len()); }
    if cfg!(duplicate_response) { unsafe { write(context, response.as_ptr(), response.len()); } }
    if !describe && cfg!(callback_error) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn not_a_plugin() {}
