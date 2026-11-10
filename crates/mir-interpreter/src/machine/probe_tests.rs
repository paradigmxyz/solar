use super::*;
use solar_interface::{ColorChoice, Session, source_map::FileName};

const LIMITS: Limits = Limits { fuel: 100, depth: 8 };

fn with_module(text: &str, f: impl FnOnce(&Session, &Module) + Send) {
    let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    session.enter(|| {
        let source = session
            .source_map()
            .new_source_file(FileName::Custom("probe.mir".into()), text)
            .unwrap();
        let module = Module::parse(&session, &source).unwrap();
        f(&session, &module);
    });
}

fn candidate(session: &Session, module: &Module, text: &str) -> Function {
    let file = session
        .source_map()
        .new_source_file(FileName::Custom("candidate.mir".into()), text)
        .unwrap();
    module.parse_function(session, &file).unwrap()
}

fn returned(value: u64) -> Outcome {
    Outcome::Return([U256::from(value)].into_iter().collect())
}

#[derive(Default)]
struct Budget {
    instructions: usize,
    results: usize,
    terminators: usize,
    maximum: usize,
}

impl Meter for Budget {
    fn instruction(&mut self, _: &Function, _: InstId, _: &dyn Fn(ValueId) -> Option<U256>) {
        self.instructions += 1;
    }

    fn terminator(&mut self, _: &Function, _: BlockId, _: &dyn Fn(ValueId) -> Option<U256>) {
        self.terminators += 1;
    }

    fn result(&mut self, _: U256) {
        self.results += 1;
    }

    fn exhausted(&self) -> bool {
        self.instructions + self.terminators >= self.maximum
    }
}

#[test]
fn coverage_memory_replacement_and_meter() {
    with_module(
        "@module Probe\n@phase lowered\nfn @f(arg0: i256) -> i256 {\n  bb0:\n    v0 = add arg0, 1\n    mstore 128, v0\n    ret v0\n}\n",
        |session, module| {
            let (id, function) = module.iter_functions().next().unwrap();
            let initial = Memory::new(7, 0);
            let run = Machine::new(module).probe(
                id,
                &[U256::from(4)],
                initial.clone(),
                None,
                LIMITS,
                &mut (),
            );
            assert_eq!(run.outcome, returned(5));
            assert_eq!(run.memory.word(128), U256::from(5));
            assert_eq!(run.memory.first_write_outside(&initial), Some(128));
            assert_eq!(run.visited.count(), 1);
            assert_eq!(run.outcomes[function.instructions().next().unwrap()], 2);
            let stopped = Machine::new(module).probe(
                id,
                &[U256::from(4)],
                initial.clone(),
                None,
                LIMITS,
                &mut Budget { maximum: 1, ..Default::default() },
            );
            assert_eq!(stopped.outcome, Outcome::Limit(Limit::Fuel));
            let body = candidate(session, module, "fn @f(arg0: i256) -> i256 { bb0: ret arg0 }");
            let replaced = Machine::with_replacement(module, id, &body).probe(
                id,
                &[U256::from(4)],
                initial,
                None,
                LIMITS,
                &mut (),
            );
            assert_eq!(replaced.outcome, returned(4));
        },
    );
}

#[test]
fn meter_stops_before_each_phi() {
    with_module(
        "@module Phis
@phase lowered
fn @f() -> i256 {
  bb0: jump bb1
  bb1:
    v0 = phi [bb0: 7]
    v1 = phi [bb0: 8]
    ret v1
}",
        |_, module| {
            let (id, function) = module.iter_functions().next().unwrap();
            let phis = function.instructions().collect::<Vec<_>>();
            for maximum in [1, 2] {
                let mut meter = Budget { maximum, ..Default::default() };
                let run =
                    Machine::new(module).probe(id, &[], Memory::zeroed(), None, LIMITS, &mut meter);
                assert_eq!(run.outcome, Outcome::Limit(Limit::Fuel));
                assert_eq!(meter.instructions, maximum - 1);
                assert_eq!(meter.results, maximum - 1);
                assert_eq!(meter.terminators, 1);
                assert_eq!(run.outcomes[phis[0]], if maximum == 2 { 2 } else { 0 });
                assert_eq!(run.outcomes[phis[1]], 0);
            }
            // The interpreter's own fuel must stop callbacks at the same boundary.
            let mut meter = Budget { maximum: 100, ..Default::default() };
            let run = Machine::new(module).probe(
                id,
                &[],
                Memory::zeroed(),
                None,
                Limits { fuel: 1, ..LIMITS },
                &mut meter,
            );
            assert_eq!(run.outcome, Outcome::Limit(Limit::Fuel));
            assert_eq!((meter.instructions, meter.results), (0, 0));
        },
    );
}

const BACKEND_MODULE: &str = "@module Backend
@phase lowered
// backend-memory-v1
// backend-stack 0 0 1,2,3 3
fn @f() -> i256 {
  bb0:
    v0 = icall @pair
    v1 = mload 32
    v2 = add v1, 32
    v3 = mload v2
    ret v3
}
fn @pair() [return_abi=[i256, i256]] { bb0: ret 7, 8 }
fn @single() -> i256 { bb0: ret 40 }
fn @other() -> i256 { bb0: ret 0 }
";

#[test]
fn replacements_do_not_reuse_backend_instruction_ids() {
    with_module(BACKEND_MODULE, |session, module| {
        let id = FunctionId::new(0);
        let original = Machine::new(module).probe(id, &[], Memory::zeroed(), None, LIMITS, &mut ());
        assert_eq!(original.outcome, returned(8));
        // The old protocol elides inst1, but here it is the candidate's add.
        let body = candidate(
            session,
            module,
            "fn @f() -> i256 { bb0: v0 = icall @single v1 = add v0, 1 ret v1 }",
        );
        let run = Machine::with_replacement(module, id, &body).probe(
            id,
            &[],
            Memory::zeroed(),
            None,
            LIMITS,
            &mut (),
        );
        assert_eq!(run.outcome, returned(41));
    });
}

#[test]
fn replacements_do_not_reuse_backend_result_ids() {
    with_module(BACKEND_MODULE, |session, module| {
        let id = FunctionId::new(0);
        // The candidate has fewer values than the old result projection names.
        let body = candidate(session, module, "fn @f() -> i256 { bb0: v0 = icall @pair ret v0 }");
        let run = Machine::with_replacement(module, id, &body).probe(
            id,
            &[],
            Memory::zeroed(),
            None,
            LIMITS,
            &mut (),
        );
        assert_eq!(run.outcome, Outcome::Unsupported("call returning several values"));
    });
}

#[test]
fn aliased_replacement_keeps_untouched_function_identity() {
    with_module(BACKEND_MODULE, |_, module| {
        let id = FunctionId::new(0);
        let other = FunctionId::new(3);
        let body = &module.functions[id];
        // A borrowed module body still has its own identity when called by that ID.
        let machine = Machine::with_replacement(module, other, body);
        let run = machine.probe(id, &[], Memory::zeroed(), None, LIMITS, &mut ());
        assert_eq!(run.outcome, returned(8));
        assert_eq!(run.visited.count(), 1);
        assert_eq!(run.outcomes[body.instructions().next().unwrap()], 2);
        // The same body used as the replacement cannot inherit the donor's layout.
        let replaced = machine.probe(other, &[], Memory::zeroed(), None, LIMITS, &mut ());
        assert_eq!(replaced.outcome, Outcome::Unsupported("call returning several values"));
    });
}

#[test]
fn initial_memory_size_obeys_limit() {
    with_module(
        "@module Size\n@phase lowered\nfn @f() -> i256 { bb0: v0 = msize ret v0 }",
        |_, module| {
            let id = FunctionId::new(0);
            for size in [MEMORY_LIMIT / 32 + 1, 1 << 59, u64::MAX] {
                let mut meter = Budget { maximum: 100, ..Default::default() };
                let run = Machine::new(module).probe(
                    id,
                    &[],
                    Memory::new(7, size),
                    None,
                    LIMITS,
                    &mut meter,
                );
                assert_eq!(run.outcome, Outcome::Limit(Limit::Memory));
                assert_eq!((meter.instructions, meter.terminators), (0, 0));
            }
            let run = Machine::new(module).probe(
                id,
                &[],
                Memory::new(7, MEMORY_LIMIT / 32),
                None,
                LIMITS,
                &mut (),
            );
            assert_eq!(run.outcome, returned(MEMORY_LIMIT));
        },
    );
}

#[test]
fn reused_memory_has_a_fresh_write_footprint() {
    with_module(
        "@module Writes
@phase lowered
fn @setup() { bb0: mstore 128, 7 ret }
fn @original() { bb0: ret }
fn @candidate() { bb0: mstore 128, 7 ret }
fn @revert() { bb0: mstore8 129, 3 revert 0, 0 }
",
        |_, module| {
            let machine = Machine::new(module);
            let setup =
                machine.probe(FunctionId::new(0), &[], Memory::zeroed(), None, LIMITS, &mut ());
            let original =
                machine.probe(FunctionId::new(1), &[], setup.memory.clone(), None, LIMITS, &mut ());
            let changed =
                machine.probe(FunctionId::new(2), &[], setup.memory.clone(), None, LIMITS, &mut ());
            assert_eq!(original.memory.word(128), U256::from(7));
            assert_eq!(changed.memory.first_difference(&original.memory), None);
            assert_eq!(changed.memory.first_write_outside(&original.memory), Some(128));
            assert_eq!(original.memory.first_write_outside(&Memory::zeroed()), None);
            let reverted =
                machine.probe(FunctionId::new(3), &[], setup.memory, None, LIMITS, &mut ());
            assert_eq!(reverted.outcome, Outcome::Revert(Vec::new()));
            assert_eq!(reverted.memory.first_write_outside(&original.memory), Some(129));
        },
    );
}

#[test]
fn expansion_zeroes_unwritten_memory() {
    with_module(
        "@module Expansion
@phase lowered
fn @f() -> i256 {
  bb0:
    v0 = mload 32
    v1 = keccak256 64, 32
    mcopy 128, 160, 32
    v2 = mload 128
    v3 = or v0, v2
    mstore 0, v3
    mstore 32, v1
    returndata 0, 64
}",
        |_, module| {
            let mut initial = Memory::new(7, 1);
            initial.set(0, U256::from(9));
            let run =
                Machine::new(module).probe(FunctionId::new(0), &[], initial, None, LIMITS, &mut ());
            let mut expected = vec![0; 32];
            expected.extend_from_slice(keccak256([0; 32]).as_slice());
            assert_eq!(run.outcome, Outcome::ReturnData(expected));
            assert_eq!(run.memory.word(160), U256::ZERO);
            assert_eq!(run.memory.size(), 6);
            assert_eq!(run.memory.word(192), U256::ZERO);
        },
    );
}

#[test]
fn garbage_words_do_not_share_selector_and_lane_draws() {
    let memory = Memory::new(7, 1025);
    let mut predictions = 0;
    let mut coupled = 0;
    for index in 0..1024 {
        let word = memory.word(index * 32);
        if word < U256::from(1) << 192 {
            continue;
        }
        let low = word.as_limbs()[0];
        let expected = match low % 8 {
            0 | 1 => U256::from((low >> 3) % (Garbage::SMALL + 1)),
            2 | 3 => {
                U256::from(EvmMemoryLayout::HEAP_START + 32 * ((low >> 3) % Garbage::ADDRESS_WORDS))
            }
            _ => continue,
        };
        predictions += 1;
        coupled += usize::from(memory.word((index + 1) * 32) == expected);
    }
    // With reused keys every such prediction is exact, rather than a chance match.
    assert!(predictions > 128);
    assert!(coupled * 4 < predictions, "{coupled} exact predictions out of {predictions}");
}

#[test]
fn recursive_probes_cover_blocks_and_results() {
    with_module(
        "@module Recursive
@phase lowered
fn @f(arg0: i256) -> i256 {
  bb0:
    v0 = eq arg0, 0
    jumpi v0, bb1, bb2
  bb1:
    v1 = add arg0, 0
    jump bb3
  bb2:
    v2 = sub arg0, 1
    v3 = icall @f, v2
    v4 = add v3, 1
    jump bb3
  bb3:
    v5 = phi [bb1: v1], [bb2: v4]
    ret v5
}",
        |session, module| {
            let id = FunctionId::new(0);
            let original = &module.functions[id];
            let replacement =
                candidate(session, module, &module.candidate_text(original).to_string());
            for machine in
                [Machine::new(module), Machine::with_replacement(module, id, &replacement)]
            {
                let run =
                    machine.probe(id, &[U256::from(2)], Memory::zeroed(), None, LIMITS, &mut ());
                assert_eq!(run.outcome, returned(2));
                assert_eq!(run.visited.count(), 4);
                // The recursive base case alone reaches bb1. Calls, arithmetic and the join
                // phi produce both zero and nonzero values across activations of this body.
                let body = machine.body(id).unwrap();
                for inst in body.instructions() {
                    let expected = match body.inst(inst).kind {
                        InstKind::Eq(..)
                        | InstKind::Sub(..)
                        | InstKind::ICall { .. }
                        | InstKind::Phi(..) => 3,
                        InstKind::Add(value, _) if matches!(body.value(value), Value::Arg(_)) => 1,
                        InstKind::Add(..) => 2,
                        _ => panic!("unexpected instruction in recursive test"),
                    };
                    assert_eq!(run.outcomes[inst], expected, "{inst:?}");
                }
            }
        },
    );
}

#[test]
fn replacement_probes_preserve_entry_heap() {
    with_module(
        "@module EntryHeap
@phase lowered
// backend-memory-v1
// backend-entry 0 0x400
fn @f() -> i256 { bb0: v0 = mload 64 ret v0 }
fn @caller() -> i256 { bb0: v0 = icall @f ret v0 }
",
        |session, module| {
            let id = FunctionId::new(0);
            let original_body = &module.functions[id];
            for text in [
                module.candidate_text(original_body).to_string(),
                "fn @f() -> i256 { bb0: v0 = mload 64 v1 = add v0, 0 ret v1 }".to_owned(),
            ] {
                let replacement = candidate(session, module, &text);
                for entry in [id, FunctionId::new(1)] {
                    let mut initial = Memory::zeroed();
                    initial.set(EvmMemoryLayout::FMP_SLOT, U256::from(128));
                    let original = Machine::new(module).probe(
                        entry,
                        &[],
                        initial.clone(),
                        None,
                        LIMITS,
                        &mut (),
                    );
                    assert_eq!(original.outcome, returned(0x400));
                    let replaced = Machine::with_replacement(module, id, &replacement).probe(
                        entry,
                        &[],
                        initial,
                        None,
                        LIMITS,
                        &mut (),
                    );
                    assert_eq!(replaced.outcome, original.outcome);
                    assert_eq!(replaced.memory.first_difference(&original.memory), None);
                    assert_eq!(replaced.memory.first_write_outside(&original.memory), None);
                    assert_eq!(replaced.memory.word(EvmMemoryLayout::FMP_SLOT), U256::from(0x400));
                }
            }
        },
    );
}
