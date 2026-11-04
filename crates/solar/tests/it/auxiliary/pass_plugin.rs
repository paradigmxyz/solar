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
        } else if cfg!(graph_bad_phase) || cfg!(graph_copy_result) {
            r#"{"api_version":1,"name":"broken","version":"1","passes":[{"name":"fail","ir":"mir","phase":"lowered","enabled_by_default":true}]}"#
        } else if cfg!(wrong_type) || cfg!(invalid_function) || cfg!(graph_bad_type) || cfg!(graph_phi) || cfg!(graph_dominance) || cfg!(graph_bad_signature) || cfg!(graph_duplicate_local) || cfg!(graph_bad_constant) || cfg!(graph_duplicate_name) || cfg!(graph_split_bounds) || cfg!(graph_void_phi) {
            r#"{"api_version":1,"name":"broken","version":"1","passes":[{"name":"fail","ir":"mir","phase":"semantic","enabled_by_default":true}]}"#
        } else {
            r#"{"api_version":1,"name":"broken","version":"1","passes":[{"name":"fail","ir":"evm-ir","phase":null,"enabled_by_default":true}]}"#
        }
    } else if cfg!(graph_copy_result) {
        r#"{"edits":[{"edit":"splice_mir_instructions","function":0,"block":0,"start":1,"remove":1,"instructions":[{"result":null,"ty":null,"instruction":{"kind":"copy","instruction":1,"operands":[{"kind":"value","id":0},{"kind":"constant","ty":{"kind":"integer","bits":256},"value":"0"}]}}]}]}"#
    } else if cfg!(graph_void_phi) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"fresh","parameters":[],"returns":{"kind":"void"},"blocks":[{"instructions":[{"result":null,"ty":null,"instruction":{"kind":"phi","incoming":[]}}],"terminator":{"kind":"return","values":[]}}]}]}]}"#
    } else if cfg!(graph_bad_phase) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"fresh","parameters":[{"kind":"slice","location":"memory"}],"returns":{"kind":"slice","location":"memory"},"blocks":[{"instructions":[],"terminator":{"kind":"return","values":[{"kind":"argument","index":0}]}}]}]}]}"#
    } else if cfg!(graph_split_bounds) {
        r#"{"edits":[{"edit":"split_mir_block","function":0,"block":0,"at":4294967295}]}"#
    } else if cfg!(graph_bad_type) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"fresh","parameters":[{"kind":"integer","bits":0}],"returns":{"kind":"void"},"blocks":[]}]}]}"#
    } else if cfg!(graph_phi) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"fresh","parameters":[],"returns":{"kind":"integer","bits":256},"blocks":[{"instructions":[],"terminator":{"kind":"jump","target":1}},{"instructions":[{"result":0,"ty":{"kind":"integer","bits":256},"instruction":{"kind":"phi","incoming":[]}}],"terminator":{"kind":"return","values":[{"kind":"local","id":0}]}}]}]}]}"#
    } else if cfg!(graph_dominance) {
        r#"{"edits":[{"edit":"splice_mir_instructions","function":0,"block":0,"start":0,"remove":0,"instructions":[{"result":0,"ty":{"kind":"integer","bits":256},"instruction":{"kind":"operation","operation":"add","operands":[{"kind":"local","id":0},{"kind":"argument","index":0}]}}]}]}"#
    } else if cfg!(graph_bad_signature) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"fresh","parameters":[],"returns":{"kind":"integer","bits":256},"blocks":[{"instructions":[{"result":0,"ty":{"kind":"integer","bits":256},"instruction":{"kind":"call","function":0,"args":[]}}],"terminator":{"kind":"return","values":[{"kind":"local","id":0}]}}]}]}]}"#
    } else if cfg!(graph_duplicate_local) {
        r#"{"edits":[{"edit":"splice_mir_instructions","function":0,"block":0,"start":0,"remove":0,"instructions":[{"result":0,"ty":{"kind":"integer","bits":256},"instruction":{"kind":"operation","operation":"add","operands":[{"kind":"argument","index":0},{"kind":"argument","index":0}]}},{"result":0,"ty":{"kind":"integer","bits":256},"instruction":{"kind":"operation","operation":"add","operands":[{"kind":"argument","index":0},{"kind":"argument","index":0}]}}]}]}"#
    } else if cfg!(graph_bad_constant) {
        r#"{"edits":[{"edit":"splice_mir_instructions","function":0,"block":0,"start":0,"remove":0,"instructions":[{"result":0,"ty":{"kind":"integer","bits":1},"instruction":{"kind":"operation","operation":"and","operands":[{"kind":"constant","ty":{"kind":"integer","bits":1},"value":"2"},{"kind":"constant","ty":{"kind":"integer","bits":1},"value":"0"}]}}]}]}"#
    } else if cfg!(graph_duplicate_name) {
        r#"{"edits":[{"edit":"add_mir_functions","functions":[{"name":"f","parameters":[],"returns":{"kind":"void"},"blocks":[{"instructions":[],"terminator":{"kind":"return","values":[]}}]}]}]}"#
    } else if cfg!(graph_bad_opcode) {
        r#"{"edits":[{"edit":"add_evm_blocks","blocks":[{"instructions":[],"terminator":{"kind":"opcode","opcode":1},"call_effect":null}]}]}"#
    } else if cfg!(graph_bad_call_effect) {
        r#"{"edits":[{"edit":"add_evm_blocks","blocks":[{"instructions":[],"terminator":{"kind":"opcode","opcode":86},"call_effect":{"ret":1,"inputs":1,"outputs":1}}]}]}"#
    } else if cfg!(graph_bad_target) {
        r#"{"edits":[{"edit":"add_evm_blocks","blocks":[{"instructions":[],"terminator":{"kind":"jump","target":4294967295},"call_effect":null}]}]}"#
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
