//! A native pass plugin that removes `add x, 0` uses in MIR and `NOT; NOT` in EVM IR.
//! Build with `cargo build -p solar-pass-plugin-example` and load the resulting shared library
//! using `-Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so` (Linux).
//! Neither pass adds instructions or assumes anything about memory, storage, or call effects.
//! Both skip modules containing gas observations, whose values can depend on removed work.

use solar_pass_plugin::{
    API_VERSION, Edit, Ir, Manifest, Pass, Request, Response, Snapshot, WriteResponse,
};
use std::ffi::c_void;

fn handle(request: Request) -> Result<Vec<u8>, String> {
    match request {
        Request::Describe { .. } => serde_json::to_vec(&Manifest {
            api_version: API_VERSION,
            name: "example".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            passes: vec![
                Pass {
                    name: "simplify-add-zero".into(),
                    ir: Ir::Mir,
                    phase: Some("semantic".into()),
                    enabled_by_default: true,
                },
                Pass {
                    name: "simplify-lowered".into(),
                    ir: Ir::Mir,
                    phase: Some("lowered".into()),
                    enabled_by_default: false,
                },
                Pass {
                    name: "remove-double-not".into(),
                    ir: Ir::EvmIr,
                    phase: None,
                    enabled_by_default: true,
                },
            ],
        })
        .map_err(|error| error.to_string()),
        Request::Run { module, pass, .. } => {
            let mut edits = Vec::new();
            match module {
                Snapshot::Mir { functions, .. }
                    if !functions.iter().any(|function| {
                        function.instructions.iter().any(|inst| inst.operation == "gas")
                    }) =>
                {
                    for function in functions {
                        for inst in &function.instructions {
                            if inst.operation != "add" || inst.operands.len() != 2 {
                                continue;
                            }
                            let Some(result) = inst.result else { continue };
                            for index in 0..2 {
                                if function.values.iter().any(|value| {
                                    value.id == inst.operands[index]
                                        && value.constant.as_deref() == Some("0x0")
                                }) {
                                    if pass == "simplify-lowered" {
                                        edits.push(Edit::RewriteMirInstruction {
                                            function: function.id,
                                            instruction: inst.id,
                                            operation: "or".into(),
                                            operands: inst.operands.clone(),
                                        });
                                    } else {
                                        edits.push(Edit::ReplaceMirValue {
                                            function: function.id,
                                            value: result,
                                            replacement: inst.operands[1 - index],
                                        });
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                Snapshot::EvmIr { blocks, .. }
                    if !blocks
                        .iter()
                        .any(|block| block.instructions.iter().any(|inst| inst.text == "gas")) =>
                {
                    for block in blocks {
                        let mut end = block.instructions.len();
                        while end >= 2 {
                            let pair = &block.instructions[end - 2..end];
                            if pair.iter().all(|inst| inst.text == "not" && !inst.protected) {
                                edits.push(Edit::SpliceEvmInstructions {
                                    block: block.id,
                                    start: (end - 2) as u32,
                                    remove: 2,
                                    instructions: Vec::new(),
                                });
                                end -= 2;
                            } else {
                                end -= 1;
                            }
                        }
                    }
                }
                _ => {}
            }
            serde_json::to_vec(&Response { edits }).map_err(|error| error.to_string())
        }
    }
}

/// Versioned C entry point. The SDK contains unwinding and copies responses into host memory.
///
/// # Safety
/// The caller must supply the buffers and writer described by `solar_pass_plugin::Entry`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn solar_pass_plugin_v1(
    input: *const u8,
    length: usize,
    write: WriteResponse,
    context: *mut c_void,
) -> u32 {
    // SAFETY: The host provides the SDK's entry-point contract.
    unsafe { solar_pass_plugin::dispatch(input, length, write, context, handle) }
}
