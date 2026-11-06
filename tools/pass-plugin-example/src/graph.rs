//! Construction examples. These intentionally add helper calls to demonstrate the API rather
//! than claim a gas or size improvement. They skip modules with gas observations.

use solar_pass_plugin::{
    self as api, Edit, EvmTerminator, MirInstructionKind as Kind, MirOperand as Operand,
    MirTerminator as Term, NewMirBlock as Block, NewMirFunction as Function,
    NewMirInstruction as Instruction, Snapshot, Type,
};

pub(super) fn run(pass: &str, module: &Snapshot) -> Option<Result<Vec<Edit>, String>> {
    match (pass, module) {
        ("split-lowered-block", Snapshot::Mir { functions, .. }) => {
            if functions
                .iter()
                .any(|function| function.instructions.iter().any(|inst| inst.operation == "gas"))
            {
                return Some(Ok(Vec::new()));
            }
            Some(Ok(functions
                .first()
                .map(|function| Edit::SplitMirBlock { function: function.id, block: 0, at: 0 })
                .into_iter()
                .collect()))
        }
        ("create-mir-helper" | "create-lowered-helper", Snapshot::Mir { functions, .. }) => {
            if functions
                .iter()
                .any(|function| function.instructions.iter().any(|inst| inst.operation == "gas"))
            {
                return Some(Ok(Vec::new()));
            }
            for function in functions {
                for inst in &function.instructions {
                    if inst.operation != "add"
                        || inst.operands.len() != 2
                        || inst.result_type.as_deref() != Some("i256")
                    {
                        continue;
                    }
                    let Some(result) = inst.result else { continue };
                    if !(0..2).any(|index| {
                        function.values.iter().any(|value| {
                            value.id == inst.operands[index]
                                && value.constant.as_deref() == Some("0x0")
                        })
                    }) {
                        continue;
                    };
                    let helper = functions.len() as u32;
                    let index = function
                        .instructions
                        .iter()
                        .filter(|other| other.block == inst.block)
                        .position(|other| other.id == inst.id)
                        .unwrap() as u32;
                    return Some(Ok(vec![
                        Edit::AddMirFunctions { functions: helpers(helper) },
                        Edit::AddMirBlocks {
                            function: helper,
                            blocks: vec![Block {
                                instructions: Vec::new(),
                                terminator: Term::Return { values: vec![local(2)] },
                            }],
                        },
                        Edit::SetMirTerminator {
                            function: helper,
                            block: 3,
                            terminator: Term::Jump { target: 4 },
                        },
                        Edit::SpliceMirInstructions {
                            function: function.id,
                            block: inst.block,
                            start: index,
                            remove: 1,
                            instructions: vec![
                                Instruction {
                                    result: Some(98),
                                    ty: Some(word()),
                                    instruction: Kind::Copy {
                                        instruction: inst.id,
                                        operands: inst
                                            .operands
                                            .iter()
                                            .map(|&id| Operand::Value { id })
                                            .collect(),
                                    },
                                },
                                Instruction {
                                    result: Some(99),
                                    ty: Some(word()),
                                    instruction: Kind::Call {
                                        function: helper,
                                        args: vec![local(98)],
                                    },
                                },
                            ],
                        },
                        Edit::ReplaceMirValueWith {
                            function: function.id,
                            value: result,
                            replacement: local(99),
                        },
                        Edit::SplitMirBlock { function: function.id, block: inst.block, at: index },
                    ]));
                }
            }
            Some(Ok(Vec::new()))
        }
        ("create-evm-helper", Snapshot::EvmIr { blocks, .. }) => {
            if blocks.iter().any(|block| block.instructions.iter().any(|inst| inst.text == "gas")) {
                return Some(Ok(Vec::new()));
            }
            for block in blocks {
                for (index, pair) in block.instructions.windows(2).enumerate() {
                    if !pair.iter().all(|inst| inst.text == "not" && !inst.protected) {
                        continue;
                    }
                    let continuation = blocks.len() as u32;
                    let helper = continuation + 1;
                    return Some(Ok(vec![
                        Edit::SplitEvmBlock { block: block.id, at: index as u32 + 2 },
                        Edit::AddEvmBlocks {
                            blocks: vec![api::NewEvmBlock {
                                instructions: vec![
                                    api::NewEvmInstruction::CopyFrom {
                                        block: block.id,
                                        index: index as u32,
                                    },
                                    api::NewEvmInstruction::CopyFrom {
                                        block: block.id,
                                        index: index as u32 + 1,
                                    },
                                    api::NewEvmInstruction::Opcode { opcode: 0x90, source: None },
                                ],
                                terminator: EvmTerminator::Opcode { opcode: 0x56 },
                                call_effect: Some(api::CallEffect {
                                    ret: 1,
                                    inputs: 2,
                                    outputs: 1,
                                }),
                            }],
                        },
                        Edit::SpliceEvmInstructions {
                            block: block.id,
                            start: index as u32,
                            remove: 2,
                            instructions: vec![
                                api::NewEvmInstruction::BlockAddress { block: continuation },
                                api::NewEvmInstruction::Opcode { opcode: 0x90, source: None },
                            ],
                        },
                        Edit::SetEvmTerminator {
                            block: block.id,
                            terminator: EvmTerminator::Jump { target: helper },
                        },
                    ]));
                }
            }
            Some(Ok(Vec::new()))
        }
        _ => None,
    }
}

fn word() -> Type {
    Type::Integer { bits: 256 }
}
fn boolean() -> Type {
    Type::Integer { bits: 1 }
}
fn arg() -> Operand {
    Operand::Argument { index: 0 }
}
fn local(id: u32) -> Operand {
    Operand::Local { id }
}
fn constant(value: &str) -> Operand {
    Operand::Constant { ty: word(), value: value.into() }
}
fn operation(id: u32, ty: Type, operation: &str, operands: Vec<Operand>) -> Instruction {
    Instruction {
        result: Some(id),
        ty: Some(ty),
        instruction: Kind::Operation { operation: operation.into(), operands },
    }
}

fn helpers(first: u32) -> Vec<Function> {
    vec![
        Function {
            name: format!("plugin_helper_{first}"),
            parameters: vec![word()],
            returns: word(),
            blocks: vec![
                Block {
                    instructions: vec![operation(0, boolean(), "eq", vec![arg(), constant("0")])],
                    terminator: Term::Branch { condition: local(0), then_block: 1, else_block: 2 },
                },
                Block { instructions: Vec::new(), terminator: Term::Jump { target: 3 } },
                Block { instructions: Vec::new(), terminator: Term::Jump { target: 3 } },
                Block {
                    instructions: vec![
                        Instruction {
                            result: Some(1),
                            ty: Some(word()),
                            instruction: Kind::Phi { incoming: vec![(1, arg()), (2, arg())] },
                        },
                        Instruction {
                            result: Some(2),
                            ty: Some(word()),
                            instruction: Kind::Call { function: first + 1, args: vec![local(1)] },
                        },
                    ],
                    terminator: Term::Return { values: vec![local(2)] },
                },
            ],
        },
        Function {
            name: format!("plugin_leaf_{}", first + 1),
            parameters: vec![word()],
            returns: word(),
            blocks: vec![
                Block { instructions: Vec::new(), terminator: Term::Jump { target: 1 } },
                Block {
                    instructions: vec![
                        Instruction {
                            result: Some(0),
                            ty: Some(word()),
                            instruction: Kind::Phi {
                                incoming: vec![(0, constant("0")), (2, local(2))],
                            },
                        },
                        operation(1, boolean(), "lt", vec![local(0), constant("2")]),
                    ],
                    terminator: Term::Branch { condition: local(1), then_block: 2, else_block: 3 },
                },
                Block {
                    instructions: vec![operation(2, word(), "add", vec![constant("1"), local(0)])],
                    terminator: Term::Jump { target: 1 },
                },
                Block {
                    instructions: Vec::new(),
                    terminator: Term::Return { values: vec![arg()] },
                },
            ],
        },
    ]
}
