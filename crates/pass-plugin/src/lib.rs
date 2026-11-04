//! Native pass plugin protocol. Only byte buffers, lengths, and C function pointers cross
//! the library boundary; compiler-owned Rust values and allocators never do.
//!
//! A library exports `solar_pass_plugin_v1` with the signature of [`Entry`]. Each call receives
//! a JSON [`Request`] and synchronously writes exactly one JSON response to [`WriteResponse`].
//! Use [`dispatch`] to implement that entry point in Rust. See the example plugin package.

use serde::{Deserialize, Serialize};
use std::{ffi::c_void, panic::AssertUnwindSafe};

/// ABI and wire-schema version understood by this SDK.
pub const API_VERSION: u32 = 1;
/// Unmangled C entry point, including its trailing null.
pub const ENTRY_SYMBOL: &[u8] = b"solar_pass_plugin_v1\0";
/// Success status for the entry point and response writer.
pub const SUCCESS: u32 = 0;
/// Failure status. The response is a UTF-8 diagnostic instead of JSON.
pub const FAILURE: u32 = 1;

/// Host writer. The plugin lends `length` readable bytes for the duration of this call.
/// It must call the writer exactly once, synchronously, on the calling thread.
pub type WriteResponse = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> u32;
/// Native entry point. Input is borrowed until return; response ownership stays with the host.
/// Neither this function nor the writer may unwind across the C boundary.
pub type Entry = unsafe extern "C" fn(*const u8, usize, WriteResponse, *mut c_void) -> u32;

/// IR layer accepted by a pass.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ir {
    Mir,
    EvmIr,
}

/// One named pass exported by a plugin.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pass {
    pub name: String,
    pub ir: Ir,
    /// MIR requires `semantic` or `lowered`. EVM IR requires no phase.
    pub phase: Option<String>,
    /// Run at the default extension point when the library is explicitly loaded.
    pub enabled_by_default: bool,
}

/// Registration response. Pass names are qualified by `name`, as `name::pass`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub name: String,
    pub version: String,
    pub passes: Vec<Pass>,
}

/// Request from the compiler to a plugin.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Describe {
        api_version: u32,
    },
    Run {
        api_version: u32,
        pass: String,
        evm_version: String,
        optimization: String,
        module: Snapshot,
    },
}

/// Read-only view. Numeric identities are invocation-local, not addresses or persistent IDs.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "ir", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Snapshot {
    Mir { name: String, phase: String, text: String, functions: Vec<Function> },
    EvmIr { name: String, text: String, blocks: Vec<EvmBlock> },
}

/// MIR function with its active instructions and values.
#[derive(Debug, Deserialize, Serialize)]
pub struct Function {
    pub id: u32,
    pub name: String,
    pub optimization: String,
    pub instructions: Vec<MirInstruction>,
    pub values: Vec<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MirInstruction {
    pub id: u32,
    pub block: u32,
    pub operation: String,
    pub operands: Vec<u32>,
    pub result: Option<u32>,
    pub result_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Value {
    pub id: u32,
    pub ty: String,
    /// A scalar constant in hexadecimal, or `None` for a nonconstant value.
    pub constant: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvmBlock {
    pub id: u32,
    pub instructions: Vec<EvmInstruction>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvmInstruction {
    pub index: u32,
    pub text: String,
    pub opcode: u8,
    pub immediate: Option<String>,
    pub protected: bool,
}

/// Edits applied sequentially to a private candidate, then verified before committing it.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub edits: Vec<Edit>,
}

/// V1 preserves the module's declarations, function signatures, and CFG. Passes can simplify
/// SSA uses, rewrite operand-only MIR operations, and splice physical EVM instruction windows.
/// Adding blocks, functions, types, and new operation definitions requires a future API.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "edit", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    ReplaceMirValue {
        function: u32,
        value: u32,
        replacement: u32,
    },
    RewriteMirInstruction {
        function: u32,
        instruction: u32,
        operation: String,
        operands: Vec<u32>,
    },
    SpliceEvmInstructions {
        block: u32,
        start: u32,
        remove: u32,
        instructions: Vec<NewEvmInstruction>,
    },
}

/// Existing instructions can be copied with their relocations. Newly constructed instructions
/// can inherit a known source instruction's debug context, or explicitly remain unmapped.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NewEvmInstruction {
    Copy { index: u32 },
    Opcode { opcode: u8, source: Option<u32> },
    Push { value: String, source: Option<u32> },
}

/// Implements a Rust entry point, containing panics when the library uses unwinding.
///
/// # Safety
/// `input` must reference `length` readable bytes. `write` and `context` must obey the host
/// writer contract. No pointers may be retained after this function returns.
pub unsafe fn dispatch(
    input: *const u8,
    length: usize,
    write: WriteResponse,
    context: *mut c_void,
    handler: impl FnOnce(Request) -> Result<Vec<u8>, String>,
) -> u32 {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: The entry-point caller lends this buffer until return.
        let bytes = unsafe { std::slice::from_raw_parts(input, length) };
        let request = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let version = match &request {
            Request::Describe { api_version } | Request::Run { api_version, .. } => *api_version,
        };
        if version != API_VERSION {
            return Err(format!("unsupported plugin API {version}"));
        }
        handler(request)
    }));
    let (status, bytes) = match result {
        Ok(Ok(bytes)) => (SUCCESS, bytes),
        Ok(Err(error)) => (FAILURE, error.into_bytes()),
        Err(_) => (FAILURE, b"plugin panicked".to_vec()),
    };
    // SAFETY: The host writer copies these bytes synchronously and does not retain them.
    let written = unsafe { write(context, bytes.as_ptr(), bytes.len()) };
    if written == SUCCESS { status } else { FAILURE }
}
