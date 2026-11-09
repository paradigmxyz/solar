//! A native pass plugin that removes `add x, 0` uses in MIR and `NOT; NOT` in EVM IR.
//! Build with `cargo build -p solar-pass-plugin-example` and load the resulting shared library
//! using `-Zload-pass-plugin=target/debug/libsolar_pass_plugin_example.so` (Linux).
//! Neither pass adds instructions or assumes anything about memory, storage, or call effects.
//! Both skip modules containing gas observations, whose values can depend on removed work.

use solar_pass_plugin::{
    API_VERSION, BuiltinBody, BuiltinInstruction as I, BuiltinOperand as O, Edit, Expansion,
    ExpansionExpr, ExpansionStmt, ExtensionKind, Ir, Manifest, Pass, Plugin, Request, Response,
    Snapshot,
};

mod graph;

struct Example;

impl Plugin for Example {
    fn lower_builtin(&self, name: &str) -> Result<solar_pass_plugin::BuiltinBody, String> {
        if name != "rotl" {
            return Err("unknown plugin builtin".into());
        }
        // shift = shift & 255
        // left = value << shift
        // right = value >> (256 - shift)
        // result = left | right
        Ok(BuiltinBody {
            instructions: vec![
                I {
                    operation: "and".into(),
                    operands: vec![O::Argument(1), O::Constant("255".into())],
                },
                I { operation: "shl".into(), operands: vec![O::Local(0), O::Argument(0)] },
                I {
                    operation: "sub".into(),
                    operands: vec![O::Constant("256".into()), O::Local(0)],
                },
                I { operation: "shr".into(), operands: vec![O::Local(2), O::Argument(0)] },
                I { operation: "or".into(), operands: vec![O::Local(1), O::Local(3)] },
            ],
            result: O::Local(4),
        })
    }

    fn frontend_manifest(&self) -> solar_pass_plugin::FrontendManifest {
        solar_pass_plugin::FrontendManifest {
            builtins: vec![solar_pass_plugin::BuiltinDefinition {
                name: "rotl".into(),
                signature: solar_pass_plugin::BuiltinSignature {
                    parameters: vec![solar_pass_plugin::BuiltinType::Uint256; 2],
                    result: solar_pass_plugin::BuiltinType::Uint256,
                    effect: solar_pass_plugin::BuiltinEffect::Pure,
                },
            }],
            extensions: vec![
                solar_pass_plugin::FrontendExtension {
                    name: "sum".into(),
                    kind: solar_pass_plugin::ExtensionKind::Expression,
                },
                solar_pass_plugin::FrontendExtension {
                    name: "repeat".into(),
                    kind: solar_pass_plugin::ExtensionKind::Statement,
                },
            ],
        }
    }

    fn expand(
        &self,
        request: &solar_pass_plugin::ExpansionRequest<'_>,
    ) -> Result<solar_pass_plugin::Expansion, String> {
        if request.name == "sum" && request.kind == ExtensionKind::Expression {
            return Ok(Expansion::Expression(ExpansionExpr::Binary {
                op: "+".into(),
                left: Box::new(ExpansionExpr::Argument(0)),
                right: Box::new(ExpansionExpr::Argument(1)),
            }));
        }
        if request.name != "repeat" || request.kind != ExtensionKind::Statement {
            return Err("unknown frontend extension".into());
        }
        Ok(Expansion::Statement(ExpansionStmt::Repeat {
            count: ExpansionExpr::Argument(0),
            body: Box::new(ExpansionStmt::Body),
        }))
    }

    fn manifest(&self) -> Manifest {
        Manifest {
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
                Pass {
                    name: "create-mir-helper".into(),
                    ir: Ir::Mir,
                    phase: Some("semantic".into()),
                    enabled_by_default: false,
                },
                Pass {
                    name: "create-lowered-helper".into(),
                    ir: Ir::Mir,
                    phase: Some("lowered".into()),
                    enabled_by_default: false,
                },
                Pass {
                    name: "create-evm-helper".into(),
                    ir: Ir::EvmIr,
                    phase: None,
                    enabled_by_default: false,
                },
                Pass {
                    name: "split-lowered-block".into(),
                    ir: Ir::Mir,
                    phase: Some("lowered".into()),
                    enabled_by_default: false,
                },
            ],
        }
    }

    fn run(&self, request: &Request) -> Result<Response, String> {
        let Request { module, pass, .. } = request;
        if let Some(edits) = graph::run(pass, module) {
            return Ok(Response { edits: edits? });
        }
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
        Ok(Response { edits })
    }
}

solar_pass_plugin::export_plugin!(Example);
