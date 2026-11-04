//! Intentionally invalid Rust plugins. Test processes select one case through their environment.
//! This crate is only a compiler dev dependency; it is not a supported plugin example.

use solar_pass_plugin::{
    self as api, __private::Exports, Edit, EvmTerminator, Manifest, MirInstructionKind as Kind,
    MirOperand as Operand, MirTerminator as Term, NewMirBlock as Block, NewMirFunction as Function,
    NewMirInstruction as Instruction, Plugin, Request, Response, Type,
};

struct Broken;

impl Plugin for Broken {
    fn manifest(&self) -> Manifest {
        let mode = mode();
        let phase = match mode.as_str() {
            "graph_bad_phase" | "graph_copy_result" => Some("lowered"),
            "wrong_type"
            | "invalid_function"
            | "graph_bad_type"
            | "graph_phi"
            | "graph_dominance"
            | "graph_bad_signature"
            | "graph_duplicate_local"
            | "graph_bad_constant"
            | "graph_duplicate_name"
            | "graph_split_bounds"
            | "graph_void_phi" => Some("semantic"),
            _ => None,
        };
        Manifest {
            api_version: if mode == "bad_version" { 999 } else { api::API_VERSION },
            name: "broken".into(),
            version: "1".into(),
            passes: vec![api::Pass {
                name: "fail".into(),
                ir: if phase.is_some() { api::Ir::Mir } else { api::Ir::EvmIr },
                phase: phase.map(str::to_owned),
                enabled_by_default: true,
            }],
        }
    }

    fn run(&self, _: &Request) -> Result<Response, String> {
        let edit = match mode().as_str() {
            "wrong_type" => Edit::ReplaceMirValue { function: 0, value: 0, replacement: 1 },
            "invalid_function" => {
                Edit::ReplaceMirValue { function: u32::MAX, value: 0, replacement: 1 }
            }
            "invalid_block" => splice_evm(u32::MAX, Vec::new()),
            "invalid_ir" => splice_evm(0, vec![opcode(0x50)]),
            "wrong_fork" => splice_evm(
                0,
                vec![
                    api::NewEvmInstruction::Push { value: "0x0".into(), source: None },
                    opcode(0x5c),
                    opcode(0x50),
                ],
            ),
            "invalid_opcode" => splice_evm(0, vec![opcode(0xe6)]),
            "graph_split_bounds" => Edit::SplitMirBlock { function: 0, block: 0, at: u32::MAX },
            "graph_bad_type" => {
                add_function("fresh", vec![Type::Integer { bits: 0 }], Type::Void, Vec::new())
            }
            "graph_void_phi" => add_function(
                "fresh",
                Vec::new(),
                Type::Void,
                vec![Block {
                    instructions: vec![Instruction {
                        result: None,
                        ty: None,
                        instruction: Kind::Phi { incoming: Vec::new() },
                    }],
                    terminator: Term::Return { values: Vec::new() },
                }],
            ),
            "graph_phi" => add_function(
                "fresh",
                Vec::new(),
                word(),
                vec![
                    Block { instructions: Vec::new(), terminator: Term::Jump { target: 1 } },
                    Block {
                        instructions: vec![instruction(Kind::Phi { incoming: Vec::new() })],
                        terminator: Term::Return { values: vec![local()] },
                    },
                ],
            ),
            "graph_dominance" => splice_mir(vec![instruction(Kind::Operation {
                operation: "add".into(),
                operands: vec![local(), arg()],
            })]),
            "graph_bad_signature" => add_function(
                "fresh",
                Vec::new(),
                word(),
                vec![Block {
                    instructions: vec![instruction(Kind::Call { function: 0, args: Vec::new() })],
                    terminator: Term::Return { values: vec![local()] },
                }],
            ),
            "graph_duplicate_local" => splice_mir(vec![
                instruction(Kind::Operation {
                    operation: "add".into(),
                    operands: vec![arg(), arg()]
                });
                2
            ]),
            "graph_bad_constant" => splice_mir(vec![Instruction {
                result: Some(0),
                ty: Some(Type::Integer { bits: 1 }),
                instruction: Kind::Operation {
                    operation: "and".into(),
                    operands: ["2", "0"]
                        .map(|value| Operand::Constant {
                            ty: Type::Integer { bits: 1 },
                            value: value.into(),
                        })
                        .into(),
                },
            }]),
            "graph_duplicate_name" => add_function(
                "f",
                Vec::new(),
                Type::Void,
                vec![Block {
                    instructions: Vec::new(),
                    terminator: Term::Return { values: Vec::new() },
                }],
            ),
            "graph_bad_phase" => add_function(
                "fresh",
                vec![Type::Slice { location: api::AddressSpace::Memory }],
                Type::Slice { location: api::AddressSpace::Memory },
                vec![Block {
                    instructions: Vec::new(),
                    terminator: Term::Return { values: vec![arg()] },
                }],
            ),
            "graph_copy_result" => Edit::SpliceMirInstructions {
                function: 0,
                block: 0,
                start: 1,
                remove: 1,
                instructions: vec![Instruction {
                    result: None,
                    ty: None,
                    instruction: Kind::Copy {
                        instruction: 1,
                        operands: vec![
                            Operand::Value { id: 0 },
                            Operand::Constant { ty: word(), value: "0".into() },
                        ],
                    },
                }],
            },
            "graph_bad_opcode" => add_evm(EvmTerminator::Opcode { opcode: 1 }, None),
            "graph_bad_call_effect" => add_evm(
                EvmTerminator::Opcode { opcode: 0x56 },
                Some(api::CallEffect { ret: 1, inputs: 1, outputs: 1 }),
            ),
            "graph_bad_target" => add_evm(EvmTerminator::Jump { target: u32::MAX }, None),
            "panic" => panic!("fixture panic"),
            _ => return Err("intentional failure".into()),
        };
        Ok(Response { edits: vec![edit] })
    }
}

#[unsafe(no_mangle)]
pub static solar_rust_pass_plugin_abi_v1: [u8; 32] = api::ABI_FINGERPRINT;

// Deliberately bypass the export macro to exercise missing and duplicate callbacks. Normal
// Rust plugins use `export_plugin!`, which always responds once and contains plugin panics.
#[unsafe(no_mangle)]
pub fn solar_rust_pass_plugin_v1() -> &'static Exports {
    static EXPORTS: Exports = Exports {
        manifest: |respond| api::__private::manifest(&Broken, respond),
        run: |request, respond| match mode().as_str() {
            "no_response" => {}
            "duplicate_response" => {
                let response = Response::default();
                respond(Ok(&response));
                respond(Ok(&response));
            }
            _ => api::__private::run(&Broken, request, respond),
        },
    };
    &EXPORTS
}

fn mode() -> String {
    std::env::var("SOLAR_TEST_PASS_PLUGIN_MODE").unwrap()
}

fn word() -> Type {
    Type::Integer { bits: 256 }
}

fn arg() -> Operand {
    Operand::Argument { index: 0 }
}

fn local() -> Operand {
    Operand::Local { id: 0 }
}

fn instruction(instruction: Kind) -> Instruction {
    Instruction { result: Some(0), ty: Some(word()), instruction }
}

fn opcode(opcode: u8) -> api::NewEvmInstruction {
    api::NewEvmInstruction::Opcode { opcode, source: None }
}

fn splice_mir(instructions: Vec<Instruction>) -> Edit {
    Edit::SpliceMirInstructions { function: 0, block: 0, start: 0, remove: 0, instructions }
}

fn splice_evm(block: u32, instructions: Vec<api::NewEvmInstruction>) -> Edit {
    Edit::SpliceEvmInstructions { block, start: 0, remove: 0, instructions }
}

fn add_function(name: &str, parameters: Vec<Type>, returns: Type, blocks: Vec<Block>) -> Edit {
    Edit::AddMirFunctions {
        functions: vec![Function { name: name.into(), parameters, returns, blocks }],
    }
}

fn add_evm(terminator: EvmTerminator, call_effect: Option<api::CallEffect>) -> Edit {
    Edit::AddEvmBlocks {
        blocks: vec![api::NewEvmBlock { instructions: Vec::new(), terminator, call_effect }],
    }
}
