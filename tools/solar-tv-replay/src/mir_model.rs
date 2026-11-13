//! Structural MIR import for translation validation, without invoking codegen.
//!
//! The compiler parser and verifier own types, value IDs and CFG structure. Instruction
//! meanings come from the operation schema's `Semantics` declarations. This exporter does
//! not evaluate operations, expand passes or assign backend addresses. Replay reparses the
//! original MIR independently; it never accepts this exported instruction recipe as input.

use serde::Deserialize;
use serde_json::{Value as Json, json};
use solar_interface::{ColorChoice, Session, source_map::FileName};
use solar_mir::{
    AllocationFailure, AllocationInitialization, ArithmeticKind, BasicBlock, Builtin, Callee,
    Function, MirType, Module, RequireKind, RevertKind, RevertReason, Semantics, Terminator, Value,
    ValueId, analysis::CallGraphInfo,
};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    version: u32,
    text: String,
    function: Option<String>,
}

pub(super) fn export() -> Result<Json, String> {
    let mut bytes = Vec::new();
    io::stdin().lock().take(2_097_153).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 2_097_152 {
        return Err("MIR model request exceeds 2 MiB".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if input.version != 1 || input.text.len() > 1_048_576 {
        return Err("unsupported MIR model request".into());
    }
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let file = sess
            .source_map()
            .new_source_file(FileName::Custom("model.mir".into()), input.text)
            .map_err(|e| e.to_string())?;
        let module = Module::parse(&sess, &file).map_err(|_| "cannot parse MIR model")?;
        solar_mir::validate(&sess.dcx, &module);
        if sess.dcx.has_errors().is_err() {
            return Err("invalid MIR model".into());
        }
        export_module(&module, input.function.as_deref())
    })
}

fn export_module(module: &Module, name: Option<&str>) -> Result<Json, String> {
    if module.backend_memory.is_some() {
        return Err("backend memory records are not direct MIR semantics".into());
    }
    let (id, body) = if let Some(name) = name {
        let mut bodies =
            module.functions.iter_enumerated().filter(|(_, f)| f.name.to_string() == name);
        let selected = bodies.next().ok_or("missing MIR function")?;
        if bodies.next().is_some() {
            return Err("ambiguous MIR function".into());
        }
        selected
    } else {
        let id = module.dispatch_entry().ok_or("MIR world needs an entry or --function")?;
        (id, &module.functions[id])
    };
    let mut reachable = CallGraphInfo::collect_internal_callees(body, module.functions.len());
    reachable.insert(id);
    let mut count = reachable.count();
    if count > 128 {
        return Err("MIR model function limit of 128".into());
    }
    let mut pending = reachable.iter().collect::<Vec<_>>();
    while let Some(caller) = pending.pop() {
        let callees = CallGraphInfo::collect_internal_callees(
            &module.functions[caller],
            module.functions.len(),
        );
        for callee in callees.iter() {
            if reachable.insert(callee) {
                count += 1;
                if count > 128 {
                    return Err("MIR model function limit of 128".into());
                }
                pending.push(callee);
            }
        }
    }
    // Bound repeated type metadata before constructing the JSON response.
    let mut type_nodes = 0;
    for owner in reachable.iter() {
        let function = &module.functions[owner];
        let mut count = |ty| -> Result<(), String> {
            type_nodes += type_cost(module, ty, 0, &mut 0)?;
            if type_nodes > 65536 {
                return Err("MIR model type budget of 65536 nodes".into());
            }
            Ok(())
        };
        for &ty in function.params.iter().chain(function.return_components()) {
            count(ty)?;
        }
        if owner == id {
            for &ty in function.params.iter().chain(function.return_components()) {
                count(ty)?;
            }
        }
        for block in &function.blocks {
            for &inst in &block.instructions {
                let inst = function.inst(inst);
                if let Some(result) = inst.result() {
                    count(function.value_ty(result).ok_or("untyped MIR result")?)?;
                }
                for value in inst.kind.operands() {
                    count(function.value_ty(value).ok_or("untyped MIR operand")?)?;
                }
                if let Some(Semantics::InsertValue(ty, ..) | Semantics::ExtractValue(ty, ..)) =
                    inst.kind.semantics()
                {
                    count(ty)?;
                }
            }
            if let Some(term) = &block.terminator {
                for value in term.operands() {
                    count(function.value_ty(value).ok_or("untyped MIR operand")?)?;
                }
            }
        }
    }
    let mut block_count = 0;
    let mut instruction_count = 0;
    let functions = reachable
        .iter()
        .map(|id| {
            let function = &module.functions[id];
            block_count += function.blocks.len();
            instruction_count += function.instructions().count();
            if block_count > 512 || instruction_count > 4096 {
                return Err("MIR model size limit".into());
            }
            export_function(module, id.index(), function)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "version": 1,
        "name": body.name.to_string(),
        "entry": module.dispatch_entry() == Some(id),
        "entry_id": id.index(),
        "params": body.params.iter().map(|&ty|value_type(module, ty)).collect::<Result<Vec<_>, _>>()?,
        "returns": body.return_components().iter().map(|&ty|value_type(module, ty)).collect::<Result<Vec<_>, _>>()?,
        "functions": functions,
    }))
}

fn export_function(module: &Module, id: usize, function: &Function) -> Result<Json, String> {
    if function.params.len() > 32 {
        return Err("MIR model parameter limit".into());
    }
    let params =
        function.params.iter().map(|&ty| value_type(module, ty)).collect::<Result<Vec<_>, _>>()?;
    let returns = function
        .return_components()
        .iter()
        .map(|&ty| value_type(module, ty))
        .collect::<Result<Vec<_>, _>>()?;
    let blocks = function
        .blocks
        .iter()
        .map(|block| export_block(module, function, block))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"id": id, "params": params, "returns": returns, "blocks": blocks}))
}

fn value_type(module: &Module, ty: MirType) -> Result<Json, String> {
    fn walk(module: &Module, ty: MirType, depth: usize, nodes: &mut usize) -> Result<Json, String> {
        *nodes += 1;
        if depth > 32 || *nodes > 256 {
            return Err("MIR aggregate type limit: depth 32, nodes 256".into());
        }
        Ok(match ty {
            MirType::Int(bits) if bits.get() <= 256 => {
                json!({"kind":"word","bits":bits.get(),"name":ty.to_string()})
            }
            MirType::Ptr(location) => {
                json!({"kind":"pointer","bits":256,"location":location.to_string(),"name":ty.to_string()})
            }
            MirType::Slice(location) => {
                *nodes += 2;
                if depth + 1 > 32 || *nodes > 256 {
                    return Err("MIR aggregate type limit: depth 32, nodes 256".into());
                }
                json!({"kind":"slice","location":location.to_string(),"name":ty.to_string()})
            }
            MirType::Struct(id) => {
                let ty = module.struct_types.get(id).ok_or("unknown MIR aggregate type")?;
                let fields = ty
                    .fields
                    .iter()
                    .map(|&ty| walk(module, ty, depth + 1, nodes))
                    .collect::<Result<Vec<_>, _>>()?;
                json!({"kind":"product","fields":fields})
            }
            _ => return Err(format!("unmodeled MIR type `{ty}`")),
        })
    }
    walk(module, ty, 0, &mut 0)
}

fn operands(module: &Module, body: &Function, args: &[ValueId]) -> Result<Vec<Json>, String> {
    args.iter().map(|&v| operand(module, body, v)).collect()
}

fn operand(module: &Module, body: &Function, id: ValueId) -> Result<Json, String> {
    let ty = value_type(module, body.value_ty(id).ok_or("untyped MIR operand")?)?;
    Ok(match body.value(id) {
        Value::Arg(index) => json!({"kind":"arg","index":index.index(),"type":ty}),
        Value::Inst(_) => json!({"kind":"local","index":id.index(),"type":ty}),
        Value::Immediate(value) => {
            json!({"kind":"constant","value":value.as_u256().ok_or("unmodeled MIR constant")?.to_string(),"type":ty})
        }
        Value::Undef(_) => json!({"kind":"undefined","type":ty}),
        Value::Error(_) => return Err("MIR error value".into()),
    })
}

fn failure_code(failure: RevertKind) -> Result<Option<u8>, String> {
    match failure {
        RevertKind::Panic(code) => Ok(Some(code as u8)),
        RevertKind::Reason(RevertReason::Empty) => Ok(None),
        _ => Err("unmodeled MIR check payload".into()),
    }
}

fn export_block(module: &Module, body: &Function, block: &BasicBlock) -> Result<Json, String> {
    let mut phis = Vec::new();
    let mut instructions = Vec::new();
    for &id in &block.instructions {
        let inst = body.inst(id);
        if inst.metadata.deferred_alloc() {
            return Err(
                "deferred allocation requires backend placement, not direct MIR semantics".into()
            );
        }
        let result = inst.result().map(|v| v.index());
        let ty = inst
            .result()
            .map(|v| value_type(module, body.value_ty(v).ok_or("untyped MIR result")?))
            .transpose()?;
        if let Some(Semantics::Phi(incoming)) = inst.kind.semantics() {
            let incoming = incoming
                .iter()
                .map(|(block, value)| Ok(json!([block.index(), operand(module, body, *value)?])))
                .collect::<Result<Vec<_>, String>>()?;
            phis.push(json!({"result": result, "type": ty, "incoming": incoming}));
            continue;
        }
        // assume pred lhs, rhs: a fact a pass relied on, undefined where it fails
        if let solar_mir::InstKind::Assume { predicate, lhs, rhs, .. } = inst.kind {
            let operation = json!({"kind": "assume", "predicate": predicate.name(),
                "args": operands(module, body, &[lhs, rhs])?});
            instructions.push(json!({"result": null, "type": null, "operation": operation}));
            continue;
        }
        let operation = export_operation(module, body, &inst.kind)?;
        instructions.push(json!({"result":result,"type":ty,"operation":operation}));
    }
    let term = match block.terminator.as_ref().ok_or("missing MIR terminator")? {
        Terminator::Jump(b) => json!({"kind":"jump","target":b.index()}),
        Terminator::Branch { condition, then_block, else_block } => {
            json!({"kind":"branch","args":operands(module, body,&[*condition])?,"then":then_block.index(),"else":else_block.index()})
        }
        Terminator::Switch { value, default, cases } => {
            let cases = cases
                .iter()
                .map(|(value, block)| Ok(json!([operand(module, body, *value)?, block.index()])))
                .collect::<Result<Vec<_>, String>>()?;
            json!({"kind": "switch", "args": operands(module, body, &[*value])?, "default": default.index(), "cases": cases})
        }
        Terminator::Return { values } => {
            json!({"kind":"return","args":operands(module, body,values)?})
        }
        Terminator::Revert { offset, size } => {
            json!({"kind":"revert","args":operands(module, body,&[*offset,*size])?})
        }
        Terminator::ReturnData { offset, size } => {
            json!({"kind":"returndata","args":operands(module, body,&[*offset,*size])?})
        }
        Terminator::TailCall { function, args } => {
            json!({"kind": "tail_call", "function": function.index(), "args": operands(module, body, args)?})
        }
        Terminator::Stop => json!({"kind":"stop"}),
        Terminator::Invalid => json!({"kind":"invalid"}),
        _ => return Err("unmodeled direct MIR terminator".into()),
    };
    Ok(json!({"phis":phis,"instructions":instructions,"terminator":term}))
}

fn export_operation(
    module: &Module,
    body: &Function,
    kind: &solar_mir::InstKind,
) -> Result<Json, String> {
    Ok(
        match kind
            .semantics()
            .ok_or_else(|| format!("unmodeled MIR operation `{}`", kind.op_def().mnemonic))?
        {
            Semantics::Opcode(name, args) => {
                json!({"kind":"opcode", "name":name,"args":operands(module, body,&args)?})
            }
            Semantics::Call(Callee::Function(function), args) => {
                json!({"kind": "call", "function": function.index(), "args": operands(module, body, args)?})
            }
            Semantics::Allocate(size, semantics) => {
                json!({"kind": "allocate", "zeroed": semantics.initialization == AllocationInitialization::Zeroed,
                    "checked": semantics.failure == AllocationFailure::Checked, "args": operands(module, body, &[size])?})
            }
            Semantics::InsertValue(ty, aggregate, index, value) => {
                json!({"kind":"insert_value","aggregate_type":value_type(module, ty)?,"index":index,"args":operands(module, body,&[aggregate,value])?})
            }
            Semantics::ExtractValue(ty, aggregate, index) => {
                json!({"kind":"extract_value","aggregate_type":value_type(module, ty)?,"index":index,"args":operands(module, body,&[aggregate])?})
            }
            Semantics::MakeSlice(location, ptr, len) => {
                json!({"kind":"make_slice","location":location.to_string(),"args":operands(module, body,&[ptr,len])?})
            }
            Semantics::FreePointer => json!({"kind": "free_pointer", "args": []}),
            Semantics::MemoryZero(offset, size) => {
                json!({"kind": "memory_zero", "args": operands(module, body, &[offset, size])?})
            }
            Semantics::PointerAdd(ptr, offset) => {
                json!({"kind": "pointer_add", "args": operands(module, body, &[ptr, offset])?})
            }
            Semantics::Word(v) => json!({"kind":"word","args":operands(module, body,&[v])?}),
            Semantics::LowBits(v, bits) => {
                json!({"kind":"low_bits","bits":bits,"args":operands(module, body,&[v])?})
            }
            Semantics::SignExtend(v, from, to) => {
                json!({"kind":"sign_extend","from":from,"to":to,"args":operands(module, body,&[v])?})
            }
            Semantics::NotEqual(a, b) => json!({"kind":"ne","args":operands(module, body,&[a,b])?}),
            Semantics::Select(c, a, b) => {
                json!({"kind":"select","args":operands(module, body,&[c,a,b])?})
            }
            Semantics::Checked(op, arithmetic, a, b) => {
                let (signed, bits) = match arithmetic {
                    ArithmeticKind::Signed(n) => (true, n),
                    ArithmeticKind::Unsigned(n) => (false, n),
                };
                json!({"kind":"checked","operation":format!("{op:?}"),"signed":signed,"bits":bits,"args":operands(module, body,&[a,b])?})
            }
            Semantics::Call(Callee::Builtin(Builtin::Check { is_zero, failure }), args) => {
                json!({"kind":"check","zero":is_zero,"panic":failure_code(*failure)?,"args":operands(module, body,args)?})
            }
            Semantics::Call(Callee::Builtin(Builtin::Require(RequireKind::EmptyString)), args) => {
                json!({"kind":"check","zero":true,"panic":null,"args":operands(module, body,args)?})
            }
            _ => {
                return Err(format!("unmodeled direct MIR operation `{}`", kind.op_def().mnemonic));
            }
        },
    )
}

/// Counts a logical type before allocating its expanded representation.
fn type_cost(
    module: &Module,
    ty: MirType,
    depth: usize,
    nodes: &mut usize,
) -> Result<usize, String> {
    *nodes += 1;
    if depth > 32 || *nodes > 256 {
        return Err("MIR aggregate type limit: depth 32, nodes 256".into());
    }
    let mut cost = 1;
    match ty {
        MirType::Struct(id) => {
            for &field in &module.struct_types.get(id).ok_or("unknown MIR aggregate type")?.fields {
                cost += type_cost(module, field, depth + 1, nodes)?;
            }
        }
        MirType::Slice(_) => {
            *nodes += 2;
            if depth + 1 > 32 || *nodes > 256 {
                return Err("MIR aggregate type limit: depth 32, nodes 256".into());
            }
            cost += 2;
        }
        _ => {}
    }
    Ok(cost)
}
