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
    AbiEncodeMode, AbiParamType, AbiType, AbiWordValidator, AllocationFailure,
    AllocationInitialization, ArithmeticKind, BasicBlock, Builtin, Callee, FrameMode,
    FrameSlotKind, Function, MemoryObjectKind, MirType, Module, PackedPart, RequireKind,
    RevertKind, Semantics, Terminator, Value, ValueId, ValueLayout, analysis::CallGraphInfo,
    memory::EvmMemoryLayout,
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
    let mut constant_bytes = 0;
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
                if let Some(Semantics::StoreStorageBytesLiteral(_, bytes)) = inst.kind.semantics() {
                    constant_bytes += bytes.len();
                    if constant_bytes > 65536 {
                        return Err("MIR constant data export budget of 64 KiB".into());
                    }
                }
                if let Some(Semantics::Packed(parts, _)) = inst.kind.semantics() {
                    constant_bytes += parts
                        .iter()
                        .map(|p| match p {
                            PackedPart::Literal(b) => b.len(),
                            _ => 0,
                        })
                        .sum::<usize>();
                    if constant_bytes > 65536 {
                        return Err("MIR constant data export budget of 64 KiB".into());
                    }
                }
                if let Some(Semantics::DataCopy(data, ..)) = inst.kind.semantics() {
                    let data = module.data.get(data.id).ok_or("unknown MIR data")?;
                    constant_bytes += data.bytes.known().map_or(0, |bytes| bytes.len());
                    if constant_bytes > 65536 {
                        return Err("MIR constant data export budget of 64 KiB".into());
                    }
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
    let reenters_selected = reachable.iter().any(|caller| {
        CallGraphInfo::collect_internal_callees(&module.functions[caller], module.functions.len())
            .contains(id)
    });
    for owner in reachable.iter() {
        let f = &module.functions[owner];
        for inst in f.instructions() {
            match f.inst(inst).kind.semantics() {
                Some(Semantics::Opcode("gas", _)) => {
                    if owner != id
                        || module.dispatch_entry() == Some(owner)
                        || f.blocks.len() != 1
                        || f.instructions().count() != 1
                        || !matches!(&f.blocks[solar_mir::BlockId::ENTRY].terminator,Some(Terminator::Return {values}) if values.as_slice()==f.inst(inst).result().as_slice())
                    {
                        return Err("GAS requires an isolated opcode window".into());
                    }
                }
                Some(Semantics::InternalFrameAddress(_)) if owner != id || reenters_selected => {
                    return Err("internal frame placement requires an activation context".into());
                }
                _ => {}
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
    // Link/placement windows retain declaration identity across a rewrite.
    let declarations = module
        .to_text()
        .to_string()
        .lines()
        .skip(1)
        .take_while(|line| !line.starts_with("fn "))
        .filter(|line| !line.starts_with("@phase "))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(json!({
        "version": 1,
        "name": body.name.to_string(),
        "layout_declarations": declarations,
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
        Terminator::RevertReturndata => json!({"kind":"revert_return_data"}),
        Terminator::SelfDestruct { recipient } => {
            json!({"kind":"selfdestruct","args":operands(module,body,&[*recipient])?})
        }
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
            Semantics::MappingSlot(key, slot) => {
                json!({"kind":"mapping_slot","args":operands(module,body,&[key,slot])?})
            }
            Semantics::MappingSlotBytes(key, slot) => {
                json!({"kind":"mapping_slot_bytes","args":operands(module,body,&[key,slot])?})
            }
            Semantics::StorageArrayDataSlot(slot) => {
                json!({"kind":"array_data_slot","args":operands(module,body,&[slot])?})
            }
            Semantics::StorageArrayElementSlot(slot, index, stride) => {
                json!({"kind":"array_element_slot","stride":stride,"args":operands(module,body,&[slot,index])?})
            }
            Semantics::DataCopy(data, dest, count) => {
                let bytes = module
                    .data
                    .get(data.id)
                    .filter(|d| d.library_relocations.is_empty())
                    .and_then(|d| d.bytes.known())
                    .ok_or("datacopy of unlinked data")?;
                json!({"kind":"data_copy","bytes":bytes.as_ref(),"offset":data.offset,"args":operands(module,body,&[dest,count])?})
            }

            Semantics::ValidateStorageBytes(header) => {
                json!({"kind":"validate_storage_bytes","args":operands(module,body,&[header])?})
            }
            Semantics::LoadStorageBytes(slot) => {
                json!({"kind":"load_storage_bytes","args":operands(module,body,&[slot])?})
            }
            Semantics::StoreStorageBytes(slot, object) => {
                json!({"kind":"store_storage_bytes","args":operands(module,body,&[slot,object])?})
            }
            Semantics::StoreStorageBytesLiteral(slot, bytes) => {
                json!({"kind":"store_storage_bytes_literal","bytes":bytes,"args":operands(module,body,&[slot])?})
            }
            Semantics::ClearStorageWords(slot, first, last) => {
                json!({"kind":"clear_storage_words","args":operands(module,body,&[slot,first,last])?})
            }
            Semantics::LoadStorageArray(slot, element, variants) => {
                let layout = match element {
                    ValueLayout::UInt(size) => json!({"kind":"uint","bits":size.bits()}),
                    ValueLayout::Int(size) => json!({"kind":"int","bits":size.bits()}),
                    ValueLayout::FixedBytes(size) => {
                        json!({"kind":"fixed_bytes","bits":size.bits()})
                    }
                    ValueLayout::MemoryObject(MemoryObjectKind::Bytes) => json!({"kind":"bytes"}),
                    _ => return Err("unsupported storage array element layout".into()),
                };
                json!({"kind":"load_storage_array","element":layout,"variants":variants,"args":operands(module,body,&[slot])?})
            }
            Semantics::FrameLoad(offset, mode, kind) => {
                json!({"kind":"frame_load","address":frame_address(offset,mode,kind)?,"slot_type":value_type(module,kind.result_type())?,"args":[]})
            }
            Semantics::FrameStore(offset, mode, kind, value) => {
                json!({"kind":"frame_store","address":frame_address(offset,mode,kind)?,"slot_type":value_type(module,kind.result_type())?,"args":operands(module,body,&[value])?})
            }
            Semantics::AddressCall(name, address, input, gas, value) => {
                let gas = gas.ok_or("implicit address-call gas requires caller pricing")?;
                let mut args = vec![address, input, gas];
                args.extend(value);
                json!({"kind":"address_call","name":name,"value":value.is_some(),"args":operands(module,body,&args)?})
            }
            Semantics::Call(Callee::Builtin(Builtin::ReturndataBytes), []) => {
                json!({"kind":"returndata_bytes","args":[]})
            }
            Semantics::Call(
                Callee::Builtin(
                    builtin @ (Builtin::Sha256 | Builtin::Ripemd160 | Builtin::EcRecover),
                ),
                args,
            ) => {
                let name = match builtin {
                    Builtin::Sha256 => "sha256",
                    Builtin::Ripemd160 => "ripemd160",
                    _ => "ecrecover",
                };
                json!({"kind":"precompile_builtin","name":name,"args":operands(module,body,args)?})
            }
            Semantics::HeapFloor => {
                json!({"kind":"layout","query":"heap_floor","function":body.name.to_string(),"args":[]})
            }
            Semantics::InternalFrameAddress(offset) => {
                json!({"kind":"layout","query":"frame_address","function":body.name.to_string(),"offset":offset,"args":[]})
            }
            Semantics::ConstructorArgs(end) => {
                json!({"kind":"layout","query":"constructor","end":end,"args":[]})
            }
            Semantics::LinkedDataSize(size) => {
                json!({"kind":"layout","query":"data_size","id":size.data.index(),"addend":size.addend,"aligned":size.aligned,"args":[]})
            }
            Semantics::LibraryAddress(id) => {
                json!({"kind":"layout","query":"library","id":id.index(),"args":[]})
            }
            Semantics::ImmutableRead(id) | Semantics::ImmutableStore(id, _) => {
                if !matches!(module.immutable_type(id),ValueLayout::UInt(size) if size.bits()==256)
                {
                    return Err("non-word immutable representation".into());
                }
                let (query, args) =
                    if let Some(Semantics::ImmutableStore(_, value)) = kind.semantics() {
                        ("immutable_store", vec![value])
                    } else {
                        ("immutable_read", vec![])
                    };
                json!({"kind":"layout","query":query,"id":id.index(),"args":operands(module,body,&args)?})
            }
            Semantics::AbiEncode(mode, base, selector, args, layout) => {
                let mut values = Vec::new();
                values.extend(base);
                values.extend(selector);
                values.extend_from_slice(args);
                let mode = match mode {
                    AbiEncodeMode::Slice => 0,
                    AbiEncodeMode::Bytes => 1,
                    AbiEncodeMode::Scratch => 2,
                };
                if layout.types.iter().all(|t| matches!(t, AbiType::Word(_) | AbiType::Function)) {
                    json!({"kind":"abi_encode","mode":mode,"base":base.is_some(),"selector":selector.is_some(),"functions":layout.types.iter().map(|t|json!(matches!(t,AbiType::Function))).collect::<Vec<_>>(),"args":operands(module,body,&values)?})
                } else {
                    let (shapes, memory_slices) = abi_encode_shapes(body, &layout.types, args)?;
                    json!({"kind":"abi_encode_tree","memory_slices":memory_slices,"mode":mode,"base":base.is_some(),"selector":selector.is_some(),"shapes":shapes,"args":operands(module,body,&values)?})
                }
            }
            Semantics::AbiDecode(data, layout) => {
                let calldata =
                    body.value_slice_location(data) == Some(solar_mir::PtrLocation::Calldata);
                if layout.types.len() <= 1 && layout.types.iter().all(AbiParamType::is_scalar_word)
                {
                    json!({"kind":"abi_decode","rule":abi_rule(layout)?,"calldata":calldata,"args":operands(module,body,&[data])?})
                } else {
                    let mut nodes = 0;
                    let shapes = layout
                        .types
                        .iter()
                        .map(|ty| abi_decode_shape(ty, 0, &mut nodes))
                        .collect::<Result<Vec<_>, _>>()?;
                    json!({"kind":"abi_decode_tree","shapes":shapes,"calldata":calldata,"args":operands(module,body,&[data])?})
                }
            }
            Semantics::ValidateAbi(value) => {
                let Value::Inst(origin) = *body.value(value) else {
                    return Err("ABI marker lacks decoder evidence".into());
                };
                // validate_abi (shr raw, 64) retains the raw function word's obligation.
                // This is evidence, not a synthesized guard: a dirty word is unsupported.
                if let Some(Semantics::Opcode("shr", shift_args)) =
                    body.inst(origin).kind.semantics()
                    && let [shift, raw] = shift_args.as_slice()
                    && body.value(*shift).as_immediate().and_then(|v| v.as_u256())
                        == Some(alloy_primitives::U256::from(64))
                {
                    json!({"kind":"validate_abi","rule":{"kind":"left","bits":192,"function":false},"args":operands(module,body,&[value,*raw])?})
                } else {
                    let Some(Semantics::AbiDecode(_, layout)) = body.inst(origin).kind.semantics()
                    else {
                        return Err("ABI marker lacks decoder evidence".into());
                    };
                    let mut rule = abi_rule(layout)?;
                    if rule.is_null() {
                        return Err("ABI marker requires a scalar decoder".into());
                    }
                    if rule["function"] == true {
                        rule = json!({"kind":"uint","bits":192,"function":false});
                    }
                    json!({"kind":"validate_abi","rule":rule,"args":operands(module,body,&[value])?})
                }
            }
            Semantics::Packed(parts, hash) => {
                if parts.len() > 2048 {
                    return Err("packed part limit of 2048".into());
                }
                let mut values = Vec::new();
                let mut shapes = Vec::new();
                let mut total = 0;
                for part in parts {
                    match part {
                        PackedPart::Literal(bytes) => {
                            total += bytes.len();
                            shapes.push(json!({"kind":"literal","bytes":bytes.as_ref()}));
                        }
                        PackedPart::Scalar { value, ty } => {
                            let size =
                                usize::from(ty.type_size().ok_or("packed scalar layout")?.bytes());
                            total += size;
                            shapes.push(json!({"kind":"scalar","index":values.len(),"size":size,"fixed":matches!(ty,ValueLayout::FixedBytes(_))}));
                            values.push(*value);
                        }
                        _ => return Err("packed reference layout".into()),
                    }
                    if total > 65536 {
                        return Err("packed bytes exceed 64 KiB".into());
                    }
                }
                json!({"kind":"packed","parts":shapes,"hash":hash,"args":operands(module,body,&values)?})
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
                let (panic, message) = match *failure {
                    RevertKind::Panic(code) => (Some(code as u8), None),
                    RevertKind::Reason(reason) => (None, reason.message().map(str::as_bytes)),
                };
                json!({"kind":"check","zero":is_zero,"panic":panic,"message":message,"args":operands(module, body,args)?})
            }
            Semantics::Call(
                Callee::Builtin(builtin @ (Builtin::CheckedAddMod | Builtin::CheckedMulMod)),
                args,
            ) => {
                json!({"kind":"checked_modular","name":if matches!(builtin,Builtin::CheckedAddMod){"addmod"}else{"mulmod"},"args":operands(module,body,args)?})
            }
            Semantics::Call(Callee::Builtin(Builtin::Erc7201), args) => {
                json!({"kind":"namespace","args":operands(module,body,args)?})
            }
            Semantics::Call(
                Callee::Builtin(builtin @ (Builtin::Send | Builtin::Transfer)),
                args,
            ) => {
                json!({"kind":"value_transfer","revert":matches!(builtin,Builtin::Transfer),"args":operands(module,body,args)?})
            }
            Semantics::Call(Callee::Builtin(Builtin::Require(kind)), args) => match kind {
                RequireKind::EmptyString => {
                    json!({"kind":"require_literal","bytes":[],"args":operands(module,body,args)?})
                }
                RequireKind::LongString(bytes) => {
                    let bytes = bytes.as_byte_str();
                    if bytes.len() > 65536 {
                        return Err("require literal exceeds 64 KiB".into());
                    }
                    json!({"kind":"require_literal","bytes":bytes.to_vec(),"args":operands(module,body,args)?})
                }
                RequireKind::ShortString => {
                    json!({"kind":"require_short","args":operands(module,body,args)?})
                }
                RequireKind::ErrorString | RequireKind::CustomError(_) => {
                    let (types, values, custom) = match kind {
                        RequireKind::ErrorString => (
                            vec![AbiType::Bytes(solar_mir::PtrLocation::Memory)],
                            args.get(1..).ok_or("require arguments missing")?,
                            false,
                        ),
                        RequireKind::CustomError(layout) => (
                            layout.types.to_vec(),
                            args.get(2..).ok_or("require selector missing")?,
                            true,
                        ),
                        _ => unreachable!(),
                    };
                    let (shapes, memory_slices) = abi_encode_shapes(body, &types, values)?;
                    json!({"kind":"require_encode","custom":custom,"shapes":shapes,"memory_slices":memory_slices,"args":operands(module,body,args)?})
                }
            },
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

fn frame_address(offset: u64, mode: FrameMode, kind: FrameSlotKind) -> Result<String, String> {
    match mode {
        FrameMode::External => {
            Ok((u128::from(offset) + u128::from(EvmMemoryLayout::HEAP_START)).to_string())
        }
        FrameMode::MultiReturn if offset == 0 && kind == FrameSlotKind::Word => {
            Ok(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT.to_string())
        }
        _ => Err("MIR frame needs backend placement".into()),
    }
}

fn abi_rule(layout: &solar_mir::AbiParamLayout) -> Result<Json, String> {
    if layout.types.is_empty() {
        return Ok(Json::Null);
    }
    let [ty] = layout.types.as_ref() else {
        return Err("ABI decoding requires zero or one scalar".into());
    };
    if !ty.is_scalar_word() {
        return Err("ABI decoding requires a scalar layout".into());
    }
    let mut rule = abi_validator(ty.word_validator());
    rule["function"] = json!(matches!(ty, AbiParamType::Scalar(ValueLayout::Function)));
    Ok(rule)
}

/// Reflects the declared ABI tree; no offsets, bounds proofs or decoder IR are exported.
fn abi_decode_shape(ty: &AbiParamType, depth: usize, nodes: &mut usize) -> Result<Json, String> {
    *nodes += 1;
    if depth > 32 || *nodes > 256 {
        return Err("ABI layout limit: depth 32, nodes 256".into());
    }
    Ok(match ty {
        AbiParamType::Scalar(_) | AbiParamType::Enum { .. } => {
            let scalar = solar_mir::AbiParamLayout::new(vec![ty.clone()]);
            json!({"kind":"scalar","rule":abi_rule(&scalar)?,"width":ty.mir_type().integer_bits().unwrap_or(256)})
        }
        AbiParamType::Bytes => json!({"kind":"bytes"}),
        AbiParamType::DynamicArray(element) => {
            json!({"kind":"array","length":null,"element":abi_decode_shape(element,depth+1,nodes)?})
        }
        AbiParamType::FixedArray { element, len } => {
            json!({"kind":"array","length":len,"element":abi_decode_shape(element,depth+1,nodes)?})
        }
        AbiParamType::Tuple(fields) => {
            json!({"kind":"tuple","fields":fields.iter().map(|ty|abi_decode_shape(ty,depth+1,nodes)).collect::<Result<Vec<_>,_>>()?})
        }
    })
}

/// Reflects recursive encoder layouts without importing the lowering implementation.
fn abi_encode_shape(ty: &AbiType, depth: usize, nodes: &mut usize) -> Result<Json, String> {
    *nodes += 1;
    if depth > 32 || *nodes > 256 {
        return Err("ABI layout limit: depth 32, nodes 256".into());
    }
    let location = |location| match location {
        solar_mir::PtrLocation::Memory => Ok(0),
        solar_mir::PtrLocation::Calldata => Ok(1),
        solar_mir::PtrLocation::Returndata => Ok(2),
        solar_mir::PtrLocation::Storage => Ok(3),
        _ => Err("ABI encoding address space".to_string()),
    };
    Ok(match ty {
        AbiType::Word(validator) => json!({"kind":"word","rule":abi_validator(*validator)}),
        AbiType::Function => json!({"kind":"function"}),
        AbiType::Bytes(place) => json!({"kind":"bytes","location":location(*place)?}),
        AbiType::DynamicArray { element, location: place } => {
            json!({"kind":"array","length":null,"location":location(*place)?,"block_copy":false,"element":abi_encode_shape(element,depth+1,nodes)?})
        }
        AbiType::FixedArray { element, len, block_copy } => {
            json!({"kind":"array","length":len,"location":0,"block_copy":block_copy,"element":abi_encode_shape(element,depth+1,nodes)?})
        }
        AbiType::Tuple(fields) => {
            json!({"kind":"tuple","fields":fields.iter().map(|ty|abi_encode_shape(ty,depth+1,nodes)).collect::<Result<Vec<_>,_>>()?})
        }
    })
}

fn abi_validator(validator: Option<AbiWordValidator>) -> Json {
    let (kind, bits) = match validator {
        None => ("any", 0),
        Some(AbiWordValidator::Unsigned(n)) => ("uint", u64::from(n)),
        Some(AbiWordValidator::LeftAligned(n)) => ("left", u64::from(n)),
        Some(AbiWordValidator::SignExtend(n)) => ("sint", (n + 1) * 8),
        Some(AbiWordValidator::Bool) => ("boolean", 0),
        Some(AbiWordValidator::EnumRange(n)) => ("enumeration", n),
    };
    json!({"kind":kind,"bits":bits,"function":false})
}

fn abi_encode_shapes(
    body: &Function,
    types: &[AbiType],
    args: &[ValueId],
) -> Result<(Vec<Json>, Vec<Json>), String> {
    if types.len() != args.len() {
        return Err("ABI tuple arity".into());
    }
    let mut nodes = 0;
    let mut shapes = Vec::new();
    let mut memory_slices = Vec::new();
    for (ty, &value) in types.iter().zip(args) {
        let location = body.value_slice_location(value);
        memory_slices.push(json!(
            location == Some(solar_mir::PtrLocation::Memory)
                && matches!(ty, AbiType::FixedArray { .. } | AbiType::Tuple(_))
        ));
        let ty = if let Some(location) = location {
            ty.clone().at_location(location)
        } else {
            ty.clone()
        };
        shapes.push(abi_encode_shape(&ty, 0, &mut nodes)?);
    }
    Ok((shapes, memory_slices))
}
