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
        if mode == "replace_library_manifest" {
            replace_library();
        }
        let phase = match mode.as_str() {
            "graph_bad_phase" | "graph_copy_result" => Some("lowered"),
            "semantic_ptradd"
            | "observe_mir"
            | "copy_alloc"
            | "raw_write"
            | "late_phi"
            | "wrong_type"
            | "invalid_function"
            | "graph_bad_type"
            | "graph_phi"
            | "graph_dominance"
            | "graph_bad_signature"
            | "graph_duplicate_local"
            | "graph_bad_constant"
            | "graph_duplicate_name"
            | "graph_split_bounds"
            | "graph_void_phi"
            | "widen_array_store" => Some("semantic"),
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

    fn run(&self, request: &Request) -> Result<Response, String> {
        let mode = mode();
        if mode.starts_with("replace_library") {
            return Ok(Response::default());
        }
        if mode.starts_with("observe_") {
            eprintln!("SNAPSHOT:{request:?}");
            return Ok(Response::default());
        }
        if mode == "semantic_ptradd" {
            let api::Snapshot::Mir { functions, .. } = &request.module else { unreachable!() };
            let function = functions
                .iter()
                .find(|function| {
                    function.instructions.iter().any(|inst| inst.operation == "ptradd")
                })
                .unwrap();
            let original =
                function.instructions.iter().find(|inst| inst.operation == "ptradd").unwrap();
            return Ok(Response {
                edits: vec![
                    Edit::SpliceMirInstructions {
                        function: function.id,
                        block: original.block,
                        start: 0,
                        remove: 1,
                        instructions: vec![Instruction {
                            result: Some(0),
                            ty: Some(Type::Pointer { location: api::AddressSpace::Memory }),
                            instruction: Kind::Operation {
                                operation: "ptradd".into(),
                                operands: vec![
                                    Operand::Value { id: original.operands[0] },
                                    Operand::Argument { index: 1 },
                                ],
                            },
                        }],
                    },
                    Edit::ReplaceMirValueWith {
                        function: function.id,
                        value: original.result.unwrap(),
                        replacement: local(),
                    },
                ],
            });
        }
        if mode == "copy_alloc" {
            let api::Snapshot::Mir { functions, .. } = &request.module else { unreachable!() };
            let function = &functions[0];
            let inst = function.instructions.iter().find(|inst| inst.operation == "alloc").unwrap();
            let at = function
                .instructions
                .iter()
                .filter(|other| other.block == inst.block)
                .position(|other| other.id == inst.id)
                .unwrap() as u32;
            return Ok(Response {
                edits: vec![
                    Edit::SpliceMirInstructions {
                        function: function.id,
                        block: inst.block,
                        start: at,
                        remove: 1,
                        instructions: vec![Instruction {
                            result: Some(0),
                            ty: Some(Type::Pointer { location: api::AddressSpace::Memory }),
                            instruction: Kind::Copy {
                                instruction: inst.id,
                                operands: inst
                                    .operands
                                    .iter()
                                    .map(|&id| Operand::Value { id })
                                    .collect(),
                            },
                        }],
                    },
                    Edit::ReplaceMirValueWith {
                        function: function.id,
                        value: inst.result.unwrap(),
                        replacement: local(),
                    },
                ],
            });
        }
        if mode == "raw_write" {
            let api::Snapshot::Mir { unsafe_assembly, functions, .. } = &request.module else {
                unreachable!()
            };
            eprintln!("UNSAFE:{unsafe_assembly}");
            if *unsafe_assembly {
                return Ok(Response::default());
            }
            return Ok(Response {
                edits: vec![Edit::SpliceMirInstructions {
                    function: functions[0].id,
                    block: 0,
                    start: 0,
                    remove: 0,
                    instructions: vec![Instruction {
                        result: None,
                        ty: None,
                        instruction: Kind::Operation {
                            operation: "mstore8".into(),
                            operands: vec![
                                Operand::Constant { ty: word(), value: "0x40".into() },
                                Operand::Constant { ty: word(), value: "0".into() },
                            ],
                        },
                    }],
                }],
            });
        }
        let edit = match mode.as_str() {
            "widen_array_store" => {
                let api::Snapshot::Mir { functions, .. } = &request.module else { unreachable!() };
                let function = functions.iter().find(|function| function.name == "write").unwrap();
                let store =
                    function.instructions.iter().find(|inst| inst.operation == "mstore").unwrap();
                let start = function
                    .instructions
                    .iter()
                    .filter(|inst| inst.block == store.block)
                    .position(|inst| inst.id == store.id)
                    .unwrap() as u32;
                // mstore address, 1 => mstore address, 257
                Edit::SpliceMirInstructions {
                    function: function.id,
                    block: store.block,
                    start,
                    remove: 1,
                    instructions: vec![Instruction {
                        result: None,
                        ty: None,
                        instruction: Kind::Copy {
                            instruction: store.id,
                            operands: vec![
                                Operand::Value { id: store.operands[0] },
                                Operand::Constant { ty: word(), value: "257".into() },
                            ],
                        },
                    }],
                }
            }
            "existing_helper_output" => Edit::SpliceEvmInstructions {
                block: 3,
                start: 0,
                remove: 0,
                instructions: vec![api::NewEvmInstruction::Opcode { opcode: 0x80, source: None }],
            },
            "unused_evm_block" => Edit::AddEvmBlocks {
                blocks: vec![api::NewEvmBlock {
                    instructions: Vec::new(),
                    call_effect: None,
                    terminator: EvmTerminator::Opcode { opcode: 0x00 },
                }],
            },
            "late_phi" => Edit::AddMirFunctions {
                functions: vec![Function {
                    name: "late_phi".into(),
                    parameters: vec![word()],
                    returns: word(),
                    blocks: vec![
                        Block { instructions: Vec::new(), terminator: Term::Jump { target: 1 } },
                        Block {
                            instructions: vec![
                                Instruction {
                                    result: Some(0),
                                    ty: Some(word()),
                                    instruction: Kind::Operation {
                                        operation: "add".into(),
                                        operands: vec![arg(), arg()],
                                    },
                                },
                                Instruction {
                                    result: Some(1),
                                    ty: Some(word()),
                                    instruction: Kind::Phi { incoming: vec![(0, arg())] },
                                },
                            ],
                            terminator: Term::Return { values: vec![Operand::Local { id: 1 }] },
                        },
                    ],
                }],
            },
            "wrong_returns" => Edit::AddEvmBlocks {
                blocks: vec![api::NewEvmBlock {
                    instructions: vec![
                        api::NewEvmInstruction::Push { value: "0".into(), source: None },
                        opcode(0x90),
                    ],
                    terminator: EvmTerminator::Opcode { opcode: 0x56 },
                    call_effect: Some(api::CallEffect { ret: 0, inputs: 1, outputs: 0 }),
                }],
            },
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
    if mode() == "replace_library" {
        replace_library();
    }
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

fn replace_library() {
    std::fs::rename(
        std::env::var_os("SOLAR_TEST_PLUGIN_REPLACEMENT").unwrap(),
        std::env::var_os("SOLAR_TEST_PLUGIN_PATH").unwrap(),
    )
    .unwrap();
}
