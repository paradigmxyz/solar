//! Queries for the passes that change how a call reaches the code: `lower-abi`, which turns an
//! external function into an argument-free wrapper that decodes its arguments from calldata and
//! encodes its results as return data, and `lower-dispatch`, which synthesizes the dispatcher that
//! routes a call to a wrapper by its selector. A query of one function cannot relate these, since
//! they change its signature or create it, so each gets a query of its own, against a reference of
//! what the ABI defines, written here independently of the passes.
//!
//! # Dispatch
//!
//! The reference routes a call over the module before `lower-dispatch`: empty calldata to
//! `receive` when there is one; calldata of at least four bytes whose first four bytes are an
//! entry's selector to that entry; anything else to `fallback`, after rejecting a nonzero
//! `callvalue` with an empty revert when it is not payable, or to an empty revert. Value is
//! checked where the phases place the check: in the dispatcher, before routing to an entry that
//! is not payable, when no entry accepts value, and otherwise in each wrapper that rejects it,
//! which the `lower-abi` query proves. The two queries together check it once on every route.
//! Calldata reads zeros past its end (`calldata-padding`), so a short call can match a selector
//! only when the selector ends in zero bytes. Each entry runs its body from the module before the
//! pass, and the dispatcher from the module after it runs its wrappers from there, both inlined at
//! depth one with the encoder's cuts (see `Encoder::cut`): a wrapper whose body the encoding stops
//! covering, at a loop or a semantic operation, ends in the same event in both when both reach that
//! point with the same values and state. So the query proves the routing (selectors, the value and
//! calldata-size checks the pass hoists into the dispatcher, the guards it removes from the
//! wrappers) without modeling what the wrappers compute beyond those points. A wrapper too large
//! to inline, or that only the dispatcher calls, gets no query of its own: this one covers it.
//!
//! # ABI
//!
//! The reference decodes an entry whose parameters are each one word (integers, booleans,
//! addresses, fixed bytes, enums), on calldata of at least four bytes, which is all the dispatcher
//! routes to an entry (`dispatched-calldata`, which the dispatch query proves): calldata shorter
//! than four bytes and the head reverts empty,
//! and so does a word that is not the canonical encoding of its type (an integer or address with
//! bits above its width, a signed integer that is not its own sign extension, a boolean above
//! one, fixed bytes with bits below their width, an enum at or above its variant count); the
//! value check comes first when the wrapper must make it, that is, when the dispatcher does not
//! check value for every entry. The entry's body then runs on the words, and each return becomes
//! return data that encodes its values one word each from offset 0: a value as is when its result
//! layout is a word, since the layout's cleanup applies to words read from memory, and cleaned to
//! its source type when the entry keeps the source types of its results. Any
//! other end of the body is the wrapper's too. The wrapper, a function of no arguments, runs from
//! the same state, and the two must end alike.
//!
//! # Limits
//!
//! - Libraries are skipped: their dispatcher reads the library's deployment address, an immutable,
//!   and their entries accept value.
//! - A dispatcher that reverts with data (`--revert-strings debug`) is skipped.
//! - An entry with a dynamic or aggregate parameter, a function-pointer parameter, or an enum,
//!   function-pointer, dynamic or aggregate result is skipped, as is a `lower-abi` query whose
//!   entry body the encoding does not cover: `lower-abi` runs on semantic MIR, so most bodies with
//!   checked arithmetic or memory objects are skipped. Cuts do not apply there, since the wrapper's
//!   body is the entry's with its blocks renumbered and its arguments replaced, so the two never
//!   reach the same point.

use super::{
    BOOL, Encoder, Kind, MEMORY, Outcome, Query, STORAGE, State, WORD, all, and, any, word,
};
use crate::mir::{
    AbiParamType, AbiType, AbiWordValidator, Callee, Function, FunctionId, InstKind, Module,
    Terminator, ValueLayout, analysis::integers::integer_mask,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use solar_sema::hir::StateMutability;
use std::fmt::Write;

/// The query that the dispatcher `entry` of `after` routes every call as the ABI defines over
/// the entries of `before`.
pub(super) fn encode_dispatch(
    before: &Module,
    after: &Module,
    entry: FunctionId,
) -> Result<Query, String> {
    if before.is_library {
        return Err("a library's dispatcher reads its deployment address".into());
    }
    let dispatcher = &after.functions[entry];
    for block in &dispatcher.blocks {
        if let Some(Terminator::Revert { size, .. }) = &block.terminator
            && dispatcher.value_u256(*size) != Some(U256::ZERO)
        {
            return Err("the dispatcher reverts with data".into());
        }
    }
    let mut routes = Vec::new();
    let (mut receive, mut fallback) = (None, None);
    for (id, func) in before.functions.iter_enumerated() {
        if func.attributes.is_receive && receive.is_none() {
            receive = Some(id);
        }
        if func.attributes.is_fallback && fallback.is_none() {
            fallback = Some(id);
        }
        if let Some(selector) = func.selector {
            routes.push((U256::from(u32::from_be_bytes(selector)), id));
        }
    }
    let entries = routes.iter().map(|&(_, id)| id).chain(receive).chain(fallback);
    if entries.into_iter().any(|id| !before.functions[id].params.is_empty()) {
        return Err("an entry that takes arguments".into());
    }

    let mut encoder =
        Encoder { inline_calls: true, cut_depth: Some(1), zeroed: true, ..Default::default() };
    let start = start(&mut encoder);
    let zero = word(U256::ZERO);
    let size = encoder.apply("input_calldatasize", &[], WORD);
    let value = encoder.apply("input_callvalue", &[], WORD);
    let first = encoder.apply("input_calldataload", &[(zero.clone(), WORD)], WORD);
    // Calldata reads zeros past its end, so a short call's selector ends in zero bytes.
    encoder.axioms.insert("calldata-padding");
    for byte in 0..4u32 {
        let high = 255 - 8 * byte;
        encoder.assert(&format!(
            "(=> (bvule {size} {}) (= ((_ extract {high} {}) {first}) #x00))",
            word(U256::from(byte)),
            high - 7
        ));
    }
    let selector = encoder.define(WORD, format!("(bvlshr {first} {})", word(U256::from(224))));
    let paid = encoder.define(BOOL, format!("(not (= {value} {zero}))"));
    let to_receive = match receive {
        Some(_) => encoder.define(BOOL, format!("(= {size} {zero})")),
        None => "false".into(),
    };
    let short = encoder.define(BOOL, format!("(bvult {size} {})", word(U256::from(4))));
    // An entry's wrapper checks value itself unless the dispatcher checks it for every entry, as
    // the `lower-abi` query proves; a fallback that rejects value gets the check here.
    let entry_paid = if dispatcher_checks_value(before) { paid.as_str() } else { "false" };
    let mut expected = Vec::new();
    let mut misses = Vec::new();
    for &(case, id) in &routes {
        let hit = format!("(= {selector} {})", word(case));
        let guard = encoder
            .define(BOOL, all(&[format!("(not {to_receive})"), format!("(not {short})"), hit]));
        misses.push(format!("(not (= {selector} {}))", word(case)));
        route(&mut encoder, before, id, (&guard, entry_paid), &start, &mut expected);
    }
    let missed = encoder.define(BOOL, any(&[short, all(&misses)]));
    let default = encoder.define(BOOL, and(&format!("(not {to_receive})"), &missed));
    match fallback {
        Some(id) => route(&mut encoder, before, id, (&default, &paid), &start, &mut expected),
        None => expected.push(empty_revert(default, &start)),
    }
    if let Some(id) = receive {
        route(&mut encoder, before, id, (&to_receive, "false"), &start, &mut expected);
    }
    let actual = encoder.run((dispatcher, &after.functions), &start, "true", None)?;
    Ok(finish(encoder, &expected, &actual))
}

/// The query that the wrapper `new` of `after` decodes, runs and encodes the external function
/// `old` of `before` as the ABI defines.
pub(super) fn encode_abi(
    before: &Module,
    old: FunctionId,
    after: &Module,
    new: FunctionId,
) -> Result<Query, String> {
    if before.is_library {
        return Err("a library entry accepts value".into());
    }
    let (func, wrapper) = (&before.functions[old], &after.functions[new]);
    let params: &[AbiParamType] = func.abi_params.as_ref().map_or(&[], |layout| &layout.types);
    if params.len() != func.params.len() {
        return Err("a parameter that is not one word".into());
    }
    let results: &[AbiType] = func.abi_returns.as_ref().map_or(&[], |layout| &layout.types);
    // The source types of the results, which say how to clean them, when `lower-abi` keeps them.
    let sources = func.abi_return_params.as_ref().map(|layout| &layout.types);
    if sources.is_some_and(|sources| sources.len() != results.len()) {
        return Err("results that are not one word each".into());
    }

    let mut encoder =
        Encoder { inline_calls: true, aliases: body_copies(before, after), ..Default::default() };
    let start = start(&mut encoder);
    encoder.entry_fmp();
    let size = encoder.apply("input_calldatasize", &[], WORD);
    let value = encoder.apply("input_callvalue", &[], WORD);
    // The dispatcher reaches a wrapper only on calldata that holds a selector, as the dispatch
    // query proves.
    encoder.axioms.insert("dispatched-calldata");
    encoder.assert(&format!("(bvuge {size} {})", word(U256::from(4))));
    let mut expected = Vec::new();
    let mut guard = "true".to_owned();
    // The wrapper rejects value when the dispatcher does not do it for every entry.
    if rejects_value(func) && !dispatcher_checks_value(before) {
        let paid = encoder.define(BOOL, format!("(not (= {value} {}))", word(U256::ZERO)));
        expected.push(empty_revert(paid.clone(), &start));
        guard = encoder.define(BOOL, format!("(not {paid})"));
    }
    let head = U256::from(4 + 32 * params.len() as u64);
    let short = encoder.define(BOOL, format!("(bvult {size} {})", word(head)));
    expected.push(empty_revert(encoder.define(BOOL, and(&guard, &short)), &start));
    guard = encoder.define(BOOL, and(&guard, &format!("(not {short})")));
    let mut args = Vec::new();
    for (index, ty) in params.iter().enumerate() {
        let offset = word(U256::from(4 + 32 * index as u64));
        let raw = encoder.apply("input_calldataload", &[(offset, WORD)], WORD);
        let bits = func.params.iter().nth(index).and_then(|ty| ty.integer_bits()).unwrap_or(256);
        if let Some(valid) = canonical(ty, &raw, bits)? {
            let invalid = encoder.define(BOOL, and(&guard, &format!("(not {valid})")));
            expected.push(empty_revert(invalid, &start));
            guard = encoder.define(BOOL, and(&guard, &valid));
        }
        // A narrow parameter holds the low bits of the word, which a valid word of a type no
        // wider than the parameter has alone, except for a signed one, which it truncates.
        let arg = match bits {
            256 => raw,
            _ => encoder.define(WORD, format!("(bvand {raw} {})", word(integer_mask(bits)))),
        };
        args.push(arg);
    }
    for outcome in encoder.run((func, &before.functions), &start, &guard, Some(&args))? {
        let Kind::Return(values) = &outcome.kind else {
            expected.push(outcome);
            continue;
        };
        if values.len() != results.len() {
            return Err("results that are not one word each".into());
        }
        let mut memory = outcome.state.memory.clone();
        for (index, (value, ty)) in values.iter().zip(results).enumerate() {
            let encoded = match (sources, ty) {
                (Some(sources), _) => cleaned(&mut encoder, &sources[index], value)?,
                // A layout's validator cleans a word read from memory; a scalar result is its
                // word.
                (None, AbiType::Word(_)) => value.clone(),
                (None, other) => return Err(format!("result type `{other:?}`")),
            };
            memory = encoder.store(&memory, &word(U256::from(32 * index as u64)), &encoded, 32);
        }
        let size = word(U256::from(32 * results.len() as u64));
        expected.push(Outcome {
            guard: outcome.guard,
            kind: Kind::ReturnData { offset: word(U256::ZERO), size },
            state: State { memory, ..outcome.state },
        });
    }
    let actual = encoder.run((wrapper, &after.functions), &start, "true", None)?;
    Ok(finish(encoder, &expected, &actual))
}

/// Whether `lower-dispatch` covers the function `id` of `after` in its dispatch query: an entry
/// of `before` that no function of `after` other than the dispatcher calls.
pub(super) fn covered_by_dispatch(before: &Function, after: &Module, id: FunctionId) -> bool {
    let entry = after.dispatch_entry();
    let is_entry =
        before.selector.is_some() || before.attributes.is_receive || before.attributes.is_fallback;
    is_entry
        && after.functions.iter_enumerated().all(|(caller, func)| {
            Some(caller) == entry
                || !func.blocks.iter().any(|block| {
                    let tail = matches!(
                        block.terminator,
                        Some(Terminator::TailCall { function, .. }) if function == id
                    );
                    tail || block.instructions.iter().any(|&inst| {
                        matches!(
                            func.inst(inst).kind,
                            InstKind::ICall { function: Callee::Function(callee), .. }
                                if callee == id
                        )
                    })
                })
        })
}

/// Testing only: the copy of `after` the validator reads, with `fault` injected, so that its
/// query must find a difference. `selector` swaps the targets of the first two cases of the
/// dispatcher's selector switch, and `decode` moves the first constant calldata read of the
/// first new ABI wrapper one word on.
pub(super) fn inject(fault: &str, pass: &str, before: &Module, after: &mut Module) {
    match (fault, pass) {
        ("selector", "lower-dispatch") => {
            let Some(entry) = after.dispatch_entry() else { return };
            let func = after.function_mut(entry);
            for block in func.blocks.iter_mut() {
                if let Some(Terminator::Switch { cases, .. }) = &mut block.terminator
                    && cases.len() >= 2
                {
                    let target = cases[0].1;
                    cases[0].1 = cases[1].1;
                    cases[1].1 = target;
                    return;
                }
            }
        }
        ("decode", "lower-abi") => {
            let wrapped = |func: &Function| {
                func.attributes.is_abi_wrapper
                    && before
                        .functions
                        .iter()
                        .any(|old| old.name == func.name && !old.attributes.is_abi_wrapper)
            };
            let Some(id) = after.functions.indices().find(|&id| wrapped(&after.functions[id]))
            else {
                return;
            };
            let func = after.function_mut(id);
            let reads: Vec<_> =
                func.blocks.iter().flat_map(|block| block.instructions.clone()).collect();
            for inst in reads {
                if let InstKind::Load(ptr, crate::mir::PtrLocation::Calldata) = func.inst(inst).kind
                    && let Some(offset) = func.value_u256(ptr)
                {
                    let moved = crate::mir::FunctionBuilder::new(func).imm(offset + U256::from(32));
                    func.inst_mut(inst).kind =
                        InstKind::Load(moved, crate::mir::PtrLocation::Calldata);
                    return;
                }
            }
        }
        _ => {}
    }
}

/// The functions of `after` that copy the body of a function of `before` under another name, as
/// `lower-abi` gives an external function called internally a copy for its internal callers, by
/// the name of the function they copy: the same code, so a call to either from the same state does
/// the same, given the queries of the callees they share.
fn body_copies(before: &Module, after: &Module) -> FxHashMap<String, String> {
    let body = |module: &Module, id: FunctionId| {
        let text = module.function_text(id).to_string();
        text.split_once('{').map(|(_, body)| body.to_owned()).unwrap_or_default()
    };
    let mut copies = FxHashMap::default();
    for (id, func) in after.functions.iter_enumerated() {
        let name = func.name.to_string();
        let Some(original) = name.strip_suffix(".body") else { continue };
        let Some(old) = before
            .functions
            .indices()
            .find(|&old| before.functions[old].name.to_string() == original)
        else {
            continue;
        };
        if body(before, old) == body(after, id) {
            copies.insert(name.clone(), original.to_owned());
        }
    }
    copies
}

/// The shared start of both versions.
fn start(encoder: &mut Encoder) -> State {
    State {
        memory: encoder.declare("mem0", MEMORY),
        storage: encoder.declare("sto0", STORAGE),
        transient: encoder.declare("tra0", STORAGE),
        world: "world0".into(),
    }
}

/// An empty revert when `guard` holds, from `state`.
fn empty_revert(guard: String, state: &State) -> Outcome {
    let zero = word(U256::ZERO);
    Outcome { guard, kind: Kind::Revert { offset: zero.clone(), size: zero }, state: state.clone() }
}

/// Runs the entry `id` of `module` when `guard` holds, rejecting value when it is not payable
/// and `paid` holds: inlined, or as the event the dispatcher's tail call is when it is not.
fn route(
    encoder: &mut Encoder,
    module: &Module,
    id: FunctionId,
    (guard, paid): (&str, &str),
    start: &State,
    outcomes: &mut Vec<Outcome>,
) {
    let func = &module.functions[id];
    let mut guard = guard.to_owned();
    if rejects_value(func) && paid != "false" {
        outcomes.push(empty_revert(encoder.define(BOOL, and(&guard, paid)), start));
        guard = encoder.define(BOOL, and(&guard, &format!("(not {paid})")));
    }
    match encoder.inline(&module.functions, id, &[], (start, &guard), None) {
        Some(ends) => outcomes.extend(ends),
        None => {
            let mut state = start.clone();
            let name = encoder.callee_event(&func.name.to_string());
            encoder.event(&name, Vec::new(), &mut state);
            outcomes.push(Outcome { guard, kind: Kind::Ended, state });
        }
    }
}

/// Asserts that some outcome of `expected` and some of `actual` both happen and differ.
fn finish(mut encoder: Encoder, expected: &[Outcome], actual: &[Outcome]) -> Query {
    let mut differences = Vec::new();
    for a in expected {
        for b in actual {
            let differ = encoder.differ(a, b);
            if differ != "false" {
                differences.push(all(&[a.guard.as_str(), b.guard.as_str(), differ.as_str()]));
            }
        }
    }
    let _ = writeln!(encoder.assertions, "(assert {})", any(&differences));
    encoder.finish()
}

/// Whether an entry rejects value: unless it is payable.
fn rejects_value(func: &Function) -> bool {
    !matches!(func.attributes.state_mutability, StateMutability::Payable)
}

/// Whether the dispatcher checks value for every entry: when no entry accepts it.
fn dispatcher_checks_value(module: &Module) -> bool {
    let mut entries = module.functions.iter().filter(|func| {
        func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback
    });
    let first = entries.next();
    first.is_some_and(rejects_value) && entries.all(rejects_value)
}

/// The condition that `raw` is the canonical encoding of a value of `ty`, which a parameter
/// of `bits` bits receives as is, or `None` when every word is.
fn canonical(ty: &AbiParamType, raw: &str, bits: u32) -> Result<Option<String>, String> {
    let zero = word(U256::ZERO);
    let fits = |width: u32| format!("(bvule {raw} {})", word(integer_mask(width)));
    let (valid, width) = match ty {
        AbiParamType::Scalar(ValueLayout::UInt(size)) => {
            let width = u32::from(size.bits());
            ((width < 256).then(|| fits(width)), width)
        }
        AbiParamType::Scalar(ValueLayout::Int(size)) => {
            let width = u32::from(size.bits());
            let extended =
                format!("((_ sign_extend {}) ((_ extract {} 0) {raw}))", 256 - width, width - 1);
            ((width < 256).then(|| format!("(= {raw} {extended})")), width)
        }
        AbiParamType::Scalar(ValueLayout::Address) => (Some(fits(160)), 160),
        AbiParamType::Scalar(ValueLayout::Bool) => (Some(fits(1)), 1),
        AbiParamType::Scalar(ValueLayout::FixedBytes(size)) => {
            let low = 256 - 8 * u32::from(size.bytes());
            let mask = word(integer_mask(low));
            ((low > 0).then(|| format!("(= (bvand {raw} {mask}) {zero})")), 256)
        }
        AbiParamType::Enum { variants, .. } => {
            let width = 256 - U256::from(variants.saturating_sub(1)).leading_zeros() as u32;
            (Some(format!("(bvult {raw} {})", word(U256::from(*variants)))), width)
        }
        other => return Err(format!("parameter type `{other}`")),
    };
    // A narrower parameter holds the word only when its type bounds the word that far.
    if bits < width {
        return Err(format!("parameter type `{ty}` in {bits} bits"));
    }
    Ok(valid)
}

/// The word that encodes `value` cleaned as `validator` cleans.
fn encoded(
    encoder: &mut Encoder,
    validator: AbiWordValidator,
    value: &str,
) -> Result<String, String> {
    let zero = word(U256::ZERO);
    Ok(match validator {
        AbiWordValidator::Unsigned(bits) => {
            encoder.define(WORD, format!("(bvand {value} {})", word(integer_mask(u32::from(bits)))))
        }
        AbiWordValidator::LeftAligned(bits) => {
            let low = integer_mask(256 - u32::from(bits));
            encoder.define(WORD, format!("(bvand {value} {})", word(!low)))
        }
        AbiWordValidator::SignExtend(byte_index) => {
            let width = 8 * (byte_index + 1);
            encoder.define(
                WORD,
                format!("((_ sign_extend {}) ((_ extract {} 0) {value}))", 256 - width, width - 1),
            )
        }
        AbiWordValidator::Bool => {
            encoder.define(WORD, format!("(ite (= {value} {zero}) {zero} {})", word(U256::ONE)))
        }
        AbiWordValidator::EnumRange(_) => return Err("an enum result".into()),
    })
}

/// The word that encodes `value` as the source type `ty`, cleaned to it.
fn cleaned(encoder: &mut Encoder, ty: &AbiParamType, value: &str) -> Result<String, String> {
    let validator = match ty {
        AbiParamType::Scalar(ValueLayout::UInt(size)) if size.bits() < 256 => {
            AbiWordValidator::Unsigned(size.bits())
        }
        AbiParamType::Scalar(ValueLayout::Int(size)) if size.bits() < 256 => {
            AbiWordValidator::SignExtend(u64::from(size.bytes()) - 1)
        }
        AbiParamType::Scalar(ValueLayout::FixedBytes(size)) if size.bytes() < 32 => {
            AbiWordValidator::LeftAligned(8 * u16::from(size.bytes()))
        }
        AbiParamType::Scalar(ValueLayout::Address) => AbiWordValidator::Unsigned(160),
        AbiParamType::Scalar(ValueLayout::Bool) => AbiWordValidator::Bool,
        AbiParamType::Scalar(
            ValueLayout::UInt(_) | ValueLayout::Int(_) | ValueLayout::FixedBytes(_),
        ) => return Ok(value.to_owned()),
        other => return Err(format!("result type `{other}`")),
    };
    encoded(encoder, validator, value)
}
