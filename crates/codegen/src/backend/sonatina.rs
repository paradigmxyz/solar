//! Lowers MIR CFGs and word operations to Sonatina IR, then links its EVM backend.
//!
//! MIR phis remain SSA phis. Comparisons use Sonatina's boolean results and widen
//! them to MIR words; branches convert words back to booleans. EVM-specific
//! arithmetic preserves zero-divisor and oversized-shift behavior. Compilation
//! currently requires Osaka. Native data symbols address embedded contracts;
//! immutable words follow runtime code and are patched during deployment.
//! Activation frames use the heap and tuple results use a reserved return area.
//! Native spill storage occupies a measured prefix below translated contract memory.
//! Recursive native frames still need a dynamic allocation protocol.

use super::evm::EvmArtifact;
use crate::mir::{
    InstKind, Module, Terminator, Value, ValueId, analysis::CallGraphInfo,
    immutable::immutable_staging_base,
};
use alloy_primitives::{U256, ruint::UintTryFrom};
use solar_config::OptimizationMode;
use sonatina_codegen::{
    isa::evm::{EvmBackend, ImmediateMaterializationMode, LateCleanupProfile},
    machinst::lower::SectionWorkModule,
    stackalloc::StackifySearchProfile,
};
use sonatina_ir::{
    AccessLoc, GlobalVariableData, GlobalVariableRef, Linkage, Signature, Type,
    builder::{FunctionBuilder, ModuleBuilder, ObjectBuilder},
    func_cursor::InstInserter,
    global_variable::GvInitializer,
    inst::{
        Inst,
        arith::{self, Add},
        cast, cmp, control_flow,
        data::{SymAddr, SymSize, SymbolRef},
        evm, logic,
    },
    ir_writer::ModuleWriter,
    isa::evm::{Evm, space::MEMORY},
    module::{FuncRef, ModuleCtx},
};
use sonatina_triple::{Architecture, EvmVersion, OperatingSystem, TargetTriple, Vendor};

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let mut base = 0;
    for _ in 0..16 {
        let translated = super::memory::translate(module, base);
        let runtime_module = lower(&translated, None, base)?;
        let mut runtime_text = String::new();
        let mut required = base;
        let mut runtime =
            assemble(runtime_module, &mut runtime_text, optimization, base, &mut required)?;
        if required > base {
            base = required;
            continue;
        }
        let immutable_references = super::alternative::append_immutable_data(module, &mut runtime);
        let deployment_module = lower(&translated, Some(&runtime), base)?;
        let mut text = String::new();
        let deployment = assemble(deployment_module, &mut text, optimization, base, &mut required)?;
        if required > base {
            base = required;
            continue;
        }
        return Ok(EvmArtifact {
            deployment,
            runtime,
            immutable_references,
            backend_ir: Some(format!("// Runtime\n{runtime_text}\n// Deployment\n{text}")),
            ..Default::default()
        });
    }
    Err("Sonatina spill layout did not stabilize".into())
}

fn assemble(
    module: sonatina_ir::Module,
    text: &mut String,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> Result<Vec<u8>, String> {
    let level = match optimization {
        OptimizationMode::None => sonatina_codegen::OptLevel::O0,
        OptimizationMode::Size => sonatina_codegen::OptLevel::Os,
        _ => sonatina_codegen::OptLevel::O2,
    };
    let mut compiler = sonatina_codegen::EvmCompile::new(module).with_opt_level(level);
    let module = compiler.optimize();
    if base != 0 {
        defer_memory_addresses(module);
    }
    let mut bytes = Vec::new();
    ModuleWriter::new(module).write(&mut bytes).map_err(|e| e.to_string())?;
    *text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let backend = EvmBackend::new(sonatina_ir::isa::evm::Evm::new(module.ctx.triple))
        .with_late_cleanup_profile(match level {
            sonatina_codegen::OptLevel::O0 => LateCleanupProfile::Off,
            sonatina_codegen::OptLevel::Os => LateCleanupProfile::Size,
            _ => LateCleanupProfile::Speed,
        })
        .with_stackify_search_profile(if matches!(level, sonatina_codegen::OptLevel::O0) {
            StackifySearchProfile::Fast
        } else {
            StackifySearchProfile::Exact
        })
        .with_immediate_materialization_mode(match level {
            sonatina_codegen::OptLevel::O0 => ImmediateMaterializationMode::Gas,
            sonatina_codegen::OptLevel::Os => ImmediateMaterializationMode::Size,
            _ => ImmediateMaterializationMode::Balanced,
        });
    let entry = module
        .funcs()
        .into_iter()
        .find(|&f| module.ctx.func_sig(f, |s| s.name() == "entry"))
        .ok_or("missing Sonatina entry")?;
    let prepared =
        backend.prepare_section(SectionWorkModule::from_roots(module, entry, &[], &[]))?;
    // NOTE: The pinned dependency keeps structured memory-plan getters private.
    // Reject changes to its snapshot format rather than underestimate native memory.
    *required = (*required).max(memory_bound(&backend.snapshot_mem_plan_detail(&prepared))?);
    if *required > base {
        return Ok(Vec::new());
    }
    let artifacts = compiler.compile().map_err(|e| format!("Sonatina codegen failed: {e:?}"))?;
    let object = artifacts.first().ok_or("Sonatina produced no object")?;
    object
        .sections
        .iter()
        .find(|(key, _)| key.0.as_str() == "code")
        .map(|(_, section)| section.bytes.clone())
        .ok_or("missing Sonatina code section".into())
}

// Each independently linked code section starts at zero. Keeping fixed memory
// addresses section-relative until linking prevents the native reservation scan
// from moving its arena above contract memory. The measured arena must still fit
// entirely below the translated contract prefix before emission is allowed.
fn defer_memory_addresses(module: &sonatina_ir::Module) {
    for fid in module.funcs() {
        module.func_store.modify(fid, |f| {
            let blocks = f.layout.iter_block().collect::<Vec<_>>();
            for block in blocks {
                let instructions = f.layout.iter_inst(block).collect::<Vec<_>>();
                for id in instructions {
                    let mut addresses = f
                        .dfg
                        .effects(id)
                        .accesses
                        .iter()
                        .filter(|access| access.space == MEMORY)
                        .filter_map(|access| match access.loc {
                            AccessLoc::LinearExact { addr, .. }
                            | AccessLoc::LinearRange { addr, .. }
                                if f.dfg.value_is_imm(addr) =>
                            {
                                Some(addr)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    addresses.sort_unstable();
                    addresses.dedup();
                    for address in addresses {
                        // zero = sym_addr .; relocated = add zero address; memory_op relocated
                        let symbol =
                            SymAddr::new_unchecked(f.inst_set(), SymbolRef::CurrentSection);
                        let zero = insert_native_value(f, id, symbol);
                        let sum = Add::new_unchecked(f.inst_set(), zero, address);
                        let relocated = insert_native_value(f, id, sum);
                        let mut inst = f.dfg.clone_inst(id);
                        inst.for_each_value_mut(&mut |value| {
                            if *value == address {
                                *value = relocated;
                            }
                        });
                        f.dfg.replace_inst(id, inst);
                    }
                }
            }
        });
    }
}

fn insert_native_value(
    f: &mut sonatina_ir::Function,
    before: sonatina_ir::InstId,
    inst: impl Inst,
) -> sonatina_ir::ValueId {
    // value = instruction
    let inst = f.dfg.make_inst(inst);
    let value = f.dfg.make_value(sonatina_ir::Value::Inst { inst, result_idx: 0, ty: Type::I256 });
    f.dfg.attach_results(inst, &[value]);
    f.layout.insert_inst_before(inst, before);
    value
}

macro_rules! word {
    ($builder:expr, $inst:path $(, $arg:expr)* $(,)?) => {{
        let is = $builder.inst_set();
        let inst = <$inst>::new_unchecked(is $(, $arg)*);
        $builder.insert_inst(inst, Type::I256)
    }};
}

macro_rules! effect {
    ($builder:expr, $inst:path $(, $arg:expr)* $(,)?) => {{
        let is = $builder.inst_set();
        let inst = <$inst>::new_unchecked(is $(, $arg)*);
        $builder.insert_inst_no_result(inst);
    }};
}

macro_rules! predicate {
    ($builder:expr, $inst:path $(, $arg:expr)* $(,)?) => {{
        let is = $builder.inst_set();
        let inst = <$inst>::new_unchecked(is $(, $arg)*);
        $builder.insert_inst(inst, Type::I1)
    }};
}

fn lower(
    module: &Module,
    runtime: Option<&[u8]>,
    base: u64,
) -> Result<sonatina_ir::Module, String> {
    let constructor = runtime.is_some();
    let isa = Evm::new(TargetTriple::new(
        Architecture::Evm,
        Vendor::Ethereum,
        OperatingSystem::Evm(EvmVersion::Osaka),
    ));
    let mut native = ModuleBuilder::new(ModuleCtx::new(&isa));
    let entry = native
        .declare_function(Signature::new_unit("entry", Linkage::Public, &[]))
        .map_err(|e| e.to_string())?;
    let root = if constructor {
        module
            .functions
            .iter_enumerated()
            .find(|(_, f)| f.attributes.is_constructor)
            .map(|(id, _)| id)
    } else {
        Some(module.dispatch_entry().ok_or("missing runtime entry")?)
    };
    let mut reachable = CallGraphInfo::new(module).reachable_callees_from(root);
    if let Some(id) = root {
        reachable.insert(id);
    }
    let mut functions = vec![None; module.functions.len()];
    for fid in reachable.iter() {
        let f = &module.functions[fid];
        let returns = if f.return_components().is_empty() { &[][..] } else { &[Type::I256][..] };
        functions[fid.index()] = Some(
            native
                .declare_function(Signature::new(
                    &format!("f{}", fid.index()),
                    Linkage::Private,
                    &vec![Type::I256; f.params.len()],
                    returns,
                ))
                .map_err(|e| e.to_string())?,
        );
    }
    let mut object = ObjectBuilder::new("Contract");
    let section = object.section("code").entry(entry);
    let mut data = Vec::with_capacity(module.data.len());
    for (id, bytes) in module.data.iter_enumerated() {
        let global = declare_data(&native, format!("d{}", id.index()), bytes.bytes.linked());
        section.data(global);
        data.push(global);
    }
    let runtime_data = runtime.map(|bytes| {
        let global = declare_data(&native, "runtime".into(), bytes);
        section.data(global);
        global
    });
    object.declare(&mut native).map_err(|e| e.to_string())?;

    let fmp = base + 64;
    let (_, heap) = super::alternative::memory_layout(module);
    let mut b = native.func_builder::<InstInserter>(entry);
    let block = b.append_block();
    b.switch_to_block(block);
    effect!(b, evm::EvmMstore, imm(&mut b, fmp), imm(&mut b, heap));
    if constructor {
        if let Some(id) = module.library_deploy_address() {
            let address = word!(b, evm::EvmAddress);
            effect!(
                b,
                evm::EvmMstore,
                imm(&mut b, base + immutable_staging_base(module) + id.index() as u64 * 32),
                address
            );
        }
        if let Some(id) = root {
            let end = word!(b, SymSize, SymbolRef::CurrentSection);
            let size = word!(b, evm::EvmCodeSize);
            let size = word!(b, arith::Sub, size, end);
            effect!(b, evm::EvmCodeCopy, imm(&mut b, base + heap), end, size);
            let end = word!(b, arith::Add, size, imm(&mut b, heap + 31));
            let end = word!(b, logic::And, end, imm(&mut b, U256::MAX - U256::from(31)));
            effect!(b, evm::EvmMstore, imm(&mut b, fmp), end);
            let args = (0..module.functions[id].params.len())
                .map(|i| word!(b, evm::EvmMload, imm(&mut b, base + heap + i as u64 * 32)))
                .collect();
            b.insert_call(functions[id.index()].unwrap(), args);
        } else if !module.is_library {
            let value = word!(b, evm::EvmCallValue);
            let zero = predicate!(b, cmp::Eq, value, imm(&mut b, 0u64));
            let deploy = b.append_block();
            let revert = b.append_block();
            effect!(b, control_flow::Br, zero, deploy, revert);
            b.switch_to_block(revert);
            effect!(b, evm::EvmRevert, imm(&mut b, 0u64), imm(&mut b, 0u64));
            b.switch_to_block(deploy);
        }
    } else if let Some(id) = root {
        b.insert_call(functions[id.index()].unwrap(), Default::default());
    }
    if let Some(runtime) = runtime {
        let source = word!(b, SymAddr, SymbolRef::Global(runtime_data.unwrap()));
        let logical = word!(b, evm::EvmMload, imm(&mut b, fmp));
        let deploy = if base == 0 {
            logical
        } else {
            let physical = word!(b, arith::Add, logical, imm(&mut b, base));
            let overflow = predicate!(b, cmp::Lt, physical, logical);
            let invalid = b.append_block();
            let copy = b.append_block();
            effect!(b, control_flow::Br, overflow, invalid, copy);
            b.switch_to_block(invalid);
            effect!(b, evm::EvmInvalid);
            b.switch_to_block(copy);
            physical
        };
        effect!(b, evm::EvmCodeCopy, deploy, source, imm(&mut b, runtime.len()));
        for (id, _) in module.iter_immutables() {
            let value = word!(
                b,
                evm::EvmMload,
                imm(&mut b, base + immutable_staging_base(module) + id.index() as u64 * 32)
            );
            let offset = runtime.len()
                - super::alternative::runtime_tail_size(module)
                - (module.immutable_count() - id.index()) * 33
                + 1;
            let address = word!(b, arith::Add, deploy, imm(&mut b, offset));
            effect!(b, evm::EvmMstore, address, value);
        }
        effect!(b, evm::EvmReturn, deploy, imm(&mut b, runtime.len()));
    } else {
        effect!(b, evm::EvmStop);
    }
    b.seal_all();
    b.finish();
    for fid in reachable.iter() {
        lower_function(module, &native, fid, &functions, &data, base, constructor)?;
    }
    Ok(native.build())
}

fn lower_function(
    module: &Module,
    native: &ModuleBuilder,
    fid: crate::mir::FunctionId,
    functions: &[Option<FuncRef>],
    data: &[GlobalVariableRef],
    base: u64,
    constructor: bool,
) -> Result<(), String> {
    let f = &module.functions[fid];
    let mut b = native.func_builder::<InstInserter>(functions[fid.index()].unwrap());
    let blocks = f.blocks.iter().map(|_| b.append_block()).collect::<Vec<_>>();
    let mut values = vec![None; f.num_values()];
    // Forward references, including loop phi inputs, are patched after every definition exists.
    for id in f.live_values() {
        if values[id.index()].is_some() {
            continue;
        }
        values[id.index()] = Some(match f.value(id) {
            Value::Immediate(value) => imm(&mut b, value.as_u256().ok_or("non-word immediate")?),
            Value::Arg(arg) if !f.params.is_empty() => b.args()[arg.index()],
            Value::Arg(_) | Value::Inst(_) => b.make_undef_value(Type::I256),
            _ => return Err("undefined value in Sonatina lowering".into()),
        });
    }
    let mut definitions = Vec::new();
    let preserve_expansion = super::memory::observes_size(module);
    let (return_base, heap) = super::alternative::memory_layout(module);
    let fmp = base + 64;
    let mut frame = None;
    for (bid, block) in f.blocks.iter_enumerated() {
        b.switch_to_block(blocks[bid.index()]);
        if bid.index() == 0 {
            if super::alternative::frame_size(f) != 0 {
                let start = word!(b, evm::EvmMload, imm(&mut b, fmp));
                frame = Some(start);
                let end =
                    word!(b, arith::Add, start, imm(&mut b, super::alternative::frame_size(f)));
                effect!(b, evm::EvmMstore, imm(&mut b, fmp), end);
            }
            if f.params.is_empty() && f.arg_indices().count() != 0 {
                if f.selector.is_none() {
                    return Err("unsupported lazy Sonatina argument".into());
                }
                let args = f
                    .arg_indices()
                    .map(|arg| word!(b, evm::EvmCalldataLoad, imm(&mut b, 4 + arg.index() * 32)))
                    .collect::<Vec<_>>();
                for id in f.live_values() {
                    if let Value::Arg(arg) = f.value(id) {
                        values[id.index()] = Some(args[arg.index()]);
                    }
                }
            }
        }
        for &iid in &block.instructions {
            let inst = f.inst(iid);
            let args = inst
                .operands()
                .iter()
                .map(|v| values[v.index()].ok_or("missing Sonatina operand"))
                .collect::<Result<Vec<_>, _>>()?;
            if preserve_expansion
                && matches!(inst.kind, InstKind::MLoad(..) | InstKind::Keccak256(..))
            {
                let length = if matches!(inst.kind, InstKind::MLoad(..)) {
                    imm(&mut b, 32u64)
                } else {
                    args[1]
                };
                effect!(b, evm::EvmMcopy, args[0], args[0], length);
            }
            let result = match &inst.kind {
                InstKind::Phi(inputs) => Some(word!(
                    b,
                    control_flow::Phi,
                    inputs
                        .iter()
                        .map(|(block, v)| (values[v.index()].unwrap(), blocks[block.index()]))
                        .collect()
                )),
                InstKind::LoadImmutable(id) => Some(if constructor {
                    word!(
                        b,
                        evm::EvmMload,
                        imm(&mut b, base + immutable_staging_base(module) + id.index() as u64 * 32)
                    )
                } else {
                    let saved = word!(b, evm::EvmMload, imm(&mut b, base));
                    let size = word!(b, evm::EvmCodeSize);
                    let offset = (module.immutable_count() - id.index()) * 33 - 1
                        + super::alternative::runtime_tail_size(module);
                    let source = word!(b, arith::Sub, size, imm(&mut b, offset));
                    effect!(b, evm::EvmCodeCopy, imm(&mut b, base), source, imm(&mut b, 32u64));
                    let value = word!(b, evm::EvmMload, imm(&mut b, base));
                    effect!(b, evm::EvmMstore, imm(&mut b, base), saved);
                    value
                }),
                InstKind::InternalFrameAddr(offset) => Some(word!(
                    b,
                    arith::Add,
                    frame.ok_or("frame address without activation frame")?,
                    imm(&mut b, *offset)
                )),
                InstKind::Select(..) => {
                    let zero = predicate!(b, cmp::Eq, args[0], imm(&mut b, 0u64));
                    let flag = word!(b, cast::Zext, zero, Type::I256);
                    let mask = word!(b, arith::Sub, flag, imm(&mut b, 1u64));
                    let delta = word!(b, logic::Xor, args[1], args[2]);
                    let delta = word!(b, logic::And, delta, mask);
                    Some(word!(b, logic::Xor, args[2], delta))
                }
                InstKind::ConstructorArgsBase => Some(imm(&mut b, heap)),
                InstKind::ConstructorArgsEnd => {
                    let end = word!(b, SymSize, SymbolRef::CurrentSection);
                    let size = word!(b, evm::EvmCodeSize);
                    let size = word!(b, arith::Sub, size, end);
                    Some(word!(b, arith::Add, imm(&mut b, heap), size))
                }
                InstKind::DataCopy(source, ..) => {
                    let address = word!(b, SymAddr, SymbolRef::Global(data[source.id.index()]));
                    let offset = word!(b, arith::Add, address, imm(&mut b, source.offset));
                    effect!(b, evm::EvmCodeCopy, args[0], offset, args[1]);
                    None
                }
                _ => operation(&mut b, module, &inst.kind, &args, functions, base)?,
            };
            if let Some(id) = f.inst_result_value(iid) {
                definitions.push((
                    values[id.index()].ok_or("missing Sonatina result")?,
                    result.ok_or("instruction without result")?,
                ));
            }
        }
        let value = |id: ValueId| values[id.index()].ok_or("missing Sonatina value");
        match block.terminator.as_ref().ok_or("missing terminator")? {
            Terminator::Jump(block) => effect!(b, control_flow::Jump, blocks[block.index()]),
            Terminator::Branch { condition, then_block, else_block } => {
                let condition = predicate!(b, cmp::Ne, value(*condition)?, imm(&mut b, 0u64));
                effect!(
                    b,
                    control_flow::Br,
                    condition,
                    blocks[then_block.index()],
                    blocks[else_block.index()]
                );
            }
            Terminator::Switch { value: v, default, cases } => {
                let cases = cases
                    .iter()
                    .map(|(v, block)| Ok((value(*v)?, blocks[block.index()])))
                    .collect::<Result<Vec<_>, String>>()?;
                effect!(b, control_flow::BrTable, value(*v)?, Some(blocks[default.index()]), cases);
            }
            Terminator::Return { values } => {
                if values.len() > 1 {
                    for (i, v) in values.iter().enumerate() {
                        effect!(
                            b,
                            evm::EvmMstore,
                            imm(&mut b, base + return_base + i as u64 * 32),
                            value(*v)?
                        );
                    }
                    effect!(b, evm::EvmMstore, imm(&mut b, base + 32), imm(&mut b, return_base));
                }
                b.insert_return_values(
                    &values.iter().take(1).map(|v| value(*v)).collect::<Result<Vec<_>, _>>()?,
                );
            }
            Terminator::TailCall { function, args } => {
                b.insert_call(
                    functions[function.index()].unwrap(),
                    args.iter().map(|v| value(*v)).collect::<Result<_, _>>()?,
                );
                effect!(b, evm::EvmStop);
            }
            Terminator::ReturnData { offset, size } => {
                effect!(b, evm::EvmReturn, value(*offset)?, value(*size)?)
            }
            Terminator::Revert { offset, size } => {
                effect!(b, evm::EvmRevert, value(*offset)?, value(*size)?)
            }
            Terminator::Stop => effect!(b, evm::EvmStop),
            Terminator::Invalid => effect!(b, evm::EvmInvalid),
            Terminator::SelfDestruct { recipient } => {
                effect!(b, evm::EvmSelfDestruct, value(*recipient)?)
            }
            Terminator::RevertReturndata => {
                let size = word!(b, evm::EvmReturnDataSize);
                effect!(b, evm::EvmReturnDataCopy, imm(&mut b, base), imm(&mut b, 0u64), size);
                effect!(b, evm::EvmRevert, imm(&mut b, base), size);
            }
        }
    }
    for (placeholder, value) in definitions {
        b.func.dfg.change_to_alias(placeholder, value);
    }
    b.seal_all();
    b.finish();
    Ok(())
}

fn declare_data(native: &ModuleBuilder, name: String, bytes: &[u8]) -> GlobalVariableRef {
    native.declare_gv(GlobalVariableData::constant(
        name,
        native.declare_array_type(Type::I8, bytes.len()),
        Linkage::Private,
        GvInitializer::make_array(
            bytes.iter().map(|b| GvInitializer::make_imm(*b as i8)).collect(),
        ),
    ))
}

fn imm<T>(b: &mut FunctionBuilder<InstInserter>, value: T) -> sonatina_ir::ValueId
where
    U256: UintTryFrom<T>,
{
    b.make_imm_value(sonatina_ir::Immediate::I256(sonatina_ir::I256::from_le_bytes(
        &U256::from(value).to_le_bytes::<32>(),
    )))
}

fn operation(
    b: &mut FunctionBuilder<InstInserter>,
    module: &Module,
    inst: &InstKind,
    args: &[sonatina_ir::ValueId],
    functions: &[Option<FuncRef>],
    base: u64,
) -> Result<Option<sonatina_ir::ValueId>, String> {
    Ok(Some(match inst {
        InstKind::DataSize(size) => imm(b, size.value(module.data[size.data].bytes.linked().len())),
        InstKind::Zext(_) | InstKind::IntToPtr(_) => word!(b, arith::Add, args[0], imm(b, 0u64)),
        InstKind::Trunc(_, bits) | InstKind::PtrToInt(_, bits) => {
            word!(b, logic::And, args[0], imm(b, U256::MAX >> (256 - bits)))
        }
        InstKind::Sext(_, 1, to) => word!(b, arith::Mul, args[0], imm(b, U256::MAX >> (256 - to))),
        InstKind::Sext(_, 160, 256) => word!(b, evm::EvmSignExtend, imm(b, 19u64), args[0]),
        InstKind::SLt(..) => {
            let flag = predicate!(b, cmp::Slt, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::SGt(..) => {
            let flag = predicate!(b, cmp::Sgt, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::Div(..) => word!(b, evm::EvmUdiv, args[0], args[1]),
        InstKind::SDiv(..) => word!(b, evm::EvmSdiv, args[0], args[1]),
        InstKind::Mod(..) => word!(b, evm::EvmUmod, args[0], args[1]),
        InstKind::SMod(..) => word!(b, evm::EvmSmod, args[0], args[1]),
        InstKind::Exp(..) => word!(b, evm::EvmExp, args[0], args[1]),
        InstKind::AddMod(..) => word!(b, evm::EvmAddMod, args[0], args[1], args[2]),
        InstKind::MulMod(..) => word!(b, evm::EvmMulMod, args[0], args[1], args[2]),
        InstKind::Byte(..) => word!(b, evm::EvmByte, args[0], args[1]),
        InstKind::SignExtend(..) => word!(b, evm::EvmSignExtend, args[0], args[1]),
        InstKind::Clz(..) => word!(b, evm::EvmClz, args[0]),
        InstKind::MLoad(..) => word!(b, evm::EvmMload, args[0]),
        InstKind::MStore(..) => {
            effect!(b, evm::EvmMstore, args[0], args[1]);
            return Ok(None);
        }
        InstKind::MStore8(..) => {
            effect!(b, evm::EvmMstore8, args[0], args[1]);
            return Ok(None);
        }
        InstKind::SLoad(..) => word!(b, evm::EvmSload, args[0]),
        InstKind::SStore(..) => {
            effect!(b, evm::EvmSstore, args[0], args[1]);
            return Ok(None);
        }
        InstKind::TLoad(..) => word!(b, evm::EvmTload, args[0]),
        InstKind::TStore(..) => {
            effect!(b, evm::EvmTstore, args[0], args[1]);
            return Ok(None);
        }
        InstKind::CalldataLoad(..) => word!(b, evm::EvmCalldataLoad, args[0]),
        InstKind::CalldataSize => word!(b, evm::EvmCalldataSize),
        InstKind::CalldataCopy(..) => {
            effect!(b, evm::EvmCalldataCopy, args[0], args[1], args[2]);
            return Ok(None);
        }
        InstKind::Caller => word!(b, evm::EvmCaller),
        InstKind::CallValue => word!(b, evm::EvmCallValue),
        InstKind::Address => word!(b, evm::EvmAddress),
        InstKind::Keccak256(..) => word!(b, evm::EvmKeccak256, args[0], args[1]),
        InstKind::CodeSize => word!(b, evm::EvmCodeSize),
        InstKind::CodeCopy(..) => {
            effect!(b, evm::EvmCodeCopy, args[0], args[1], args[2]);
            return Ok(None);
        }
        InstKind::ExtCodeSize(..) => word!(b, evm::EvmExtCodeSize, args[0]),
        InstKind::ExtCodeCopy(..) => {
            effect!(b, evm::EvmExtCodeCopy, args[0], args[1], args[2], args[3]);
            return Ok(None);
        }
        InstKind::ExtCodeHash(..) => word!(b, evm::EvmExtCodeHash, args[0]),
        InstKind::ReturnDataSize => word!(b, evm::EvmReturnDataSize),
        InstKind::ReturnDataCopy(..) => {
            effect!(b, evm::EvmReturnDataCopy, args[0], args[1], args[2]);
            return Ok(None);
        }
        InstKind::Origin => word!(b, evm::EvmOrigin),
        InstKind::GasPrice => word!(b, evm::EvmGasPrice),
        InstKind::BlockHash(..) => word!(b, evm::EvmBlockHash, args[0]),
        InstKind::Coinbase => word!(b, evm::EvmCoinBase),
        InstKind::Timestamp => word!(b, evm::EvmTimestamp),
        InstKind::BlockNumber => word!(b, evm::EvmNumber),
        InstKind::PrevRandao => word!(b, evm::EvmPrevRandao),
        InstKind::GasLimit => word!(b, evm::EvmGasLimit),
        InstKind::ChainId => word!(b, evm::EvmChainId),
        InstKind::Balance(..) => word!(b, evm::EvmBalance, args[0]),
        InstKind::SelfBalance => word!(b, evm::EvmSelfBalance),
        InstKind::Gas => word!(b, evm::EvmGas),
        InstKind::BaseFee => word!(b, evm::EvmBaseFee),
        InstKind::BlobBaseFee => word!(b, evm::EvmBlobBaseFee),
        InstKind::BlobHash(..) => word!(b, evm::EvmBlobHash, args[0]),
        InstKind::MSize => word!(b, evm::EvmMsize),
        InstKind::MCopy(..) => {
            effect!(b, evm::EvmMcopy, args[0], args[1], args[2]);
            return Ok(None);
        }
        InstKind::Create(..) => word!(b, evm::EvmCreate, args[0], args[1], args[2]),
        InstKind::Create2(..) => word!(b, evm::EvmCreate2, args[0], args[1], args[2], args[3]),
        InstKind::Call { .. } => {
            word!(b, evm::EvmCall, args[0], args[1], args[2], args[3], args[4], args[5], args[6])
        }
        InstKind::CallCode { .. } => word!(
            b,
            evm::EvmCallCode,
            args[0],
            args[1],
            args[2],
            args[3],
            args[4],
            args[5],
            args[6]
        ),
        InstKind::DelegateCall { .. } => {
            word!(b, evm::EvmDelegateCall, args[0], args[1], args[2], args[3], args[4], args[5])
        }
        InstKind::StaticCall { .. } => {
            word!(b, evm::EvmStaticCall, args[0], args[1], args[2], args[3], args[4], args[5])
        }
        InstKind::Log0(..) => {
            effect!(b, evm::EvmLog0, args[0], args[1]);
            return Ok(None);
        }
        InstKind::Log1(..) => {
            effect!(b, evm::EvmLog1, args[0], args[1], args[2]);
            return Ok(None);
        }
        InstKind::Log2(..) => {
            effect!(b, evm::EvmLog2, args[0], args[1], args[2], args[3]);
            return Ok(None);
        }
        InstKind::Log3(..) => {
            effect!(b, evm::EvmLog3, args[0], args[1], args[2], args[3], args[4]);
            return Ok(None);
        }
        InstKind::Log4(..) => {
            effect!(b, evm::EvmLog4, args[0], args[1], args[2], args[3], args[4], args[5]);
            return Ok(None);
        }
        InstKind::Add(..) => word!(b, arith::Add, args[0], args[1]),
        InstKind::Sub(..) => word!(b, arith::Sub, args[0], args[1]),
        InstKind::Mul(..) => word!(b, arith::Mul, args[0], args[1]),
        InstKind::And(..) => word!(b, logic::And, args[0], args[1]),
        InstKind::Or(..) => word!(b, logic::Or, args[0], args[1]),
        InstKind::Xor(..) => word!(b, logic::Xor, args[0], args[1]),
        InstKind::Not(..) => word!(b, logic::Not, args[0]),
        InstKind::Shl(..) => word!(b, arith::Shl, args[0], args[1]),
        InstKind::Shr(..) => word!(b, arith::Shr, args[0], args[1]),
        InstKind::Sar(..) => word!(b, arith::Sar, args[0], args[1]),
        InstKind::Lt(..) => {
            let flag = predicate!(b, cmp::Lt, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::Gt(..) => {
            let flag = predicate!(b, cmp::Gt, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::Eq(..) => {
            let flag = predicate!(b, cmp::Eq, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::Ne(..) => {
            let flag = predicate!(b, cmp::Ne, args[0], args[1]);
            word!(b, cast::Zext, flag, Type::I256)
        }
        InstKind::Fmp => word!(b, evm::EvmMload, imm(b, base + 64)),
        InstKind::SetFmp(..) => {
            effect!(b, evm::EvmMstore, imm(b, base + 64), args[0]);
            return Ok(None);
        }
        InstKind::ICall { function: crate::mir::Callee::Function(function), .. } => {
            return Ok(b.insert_call(
                functions[function.index()].unwrap(),
                args.iter().copied().collect(),
            ));
        }
        _ => return Err(format!("unsupported Sonatina instruction `{}`", inst.mnemonic())),
    }))
}

fn memory_bound(snapshot: &str) -> Result<u64, String> {
    let invalid = || "unrecognized Sonatina memory plan snapshot".to_string();
    let mut lines = snapshot.lines();
    let header = lines.next().ok_or_else(invalid)?.split_whitespace().collect::<Vec<_>>();
    let ["evm", "mem", "plan:", bound, arena, scratch, stable] = header.as_slice() else {
        return Err(invalid());
    };
    let bound = snapshot_number(bound, "global_dyn_base=0x", 16)?;
    let arena = snapshot_number(arena, "arena_base=0x", 16)?;
    let scratch = snapshot_number(scratch, "scratch_peak_words=", 10)?;
    let stable = snapshot_number(stable, "stable_chain_peak_words=", 10)?;
    if scratch
        .checked_add(stable)
        .and_then(|words| words.checked_mul(32))
        .and_then(|bytes| arena.checked_add(bytes))
        != Some(bound)
    {
        return Err(invalid());
    }
    let mut functions = 0;
    let mut spills = false;
    let mut dynamic = false;
    for line in lines {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        match fields.as_slice() {
            ["evm", "mem", "plan:", _, scratch, stable, mode, entry, end] => {
                snapshot_number(scratch, "scratch_words=", 10)?;
                snapshot_number(stable, "stable_words=", 10)?;
                snapshot_number(entry, "entry_abs_words=", 10)?;
                snapshot_number(end, "abs_words_end=", 10)?;
                match *mode {
                    "stable_mode=None" => {}
                    "stable_mode=DynamicFrame" => dynamic = true,
                    mode => {
                        snapshot_number(
                            mode.strip_suffix(')').ok_or_else(invalid)?,
                            "stable_mode=StableAbs(base=0x",
                            16,
                        )?;
                    }
                }
                functions += 1;
            }
            ["scratch_spill", value, slot, address] if functions != 0 => {
                snapshot_number(value, "v", 10)?;
                snapshot_number(slot, "slot=", 10)?;
                snapshot_number(address, "addr=0x", 16)?;
                spills = true;
            }
            ["spill", value, offset, location, address] if functions != 0 => {
                snapshot_number(value, "v", 10)?;
                snapshot_number(offset, "offset_words=", 10)?;
                let location = location
                    .strip_prefix("loc=")
                    .and_then(|s| s.strip_suffix(')'))
                    .ok_or_else(invalid)?;
                let (kind, offset) = location.split_once('(').ok_or_else(invalid)?;
                if !matches!(kind, "ScratchAbs" | "StableAbs" | "StableFrame") {
                    return Err(invalid());
                }
                snapshot_number(offset, "", 10)?;
                snapshot_number(
                    address,
                    if kind == "StableFrame" { "addr=sp-0x" } else { "addr=0x" },
                    16,
                )?;
                spills = true;
            }
            ["call", inst, callee, "preserve=ShadowRuns", shadow, results, saved, runs @ ..]
                if functions != 0 =>
            {
                snapshot_number(inst, "inst", 10)?;
                if callee.strip_prefix("callee=%").is_none_or(str::is_empty)
                    || runs.first().is_none_or(|s| !s.starts_with("runs=["))
                    || runs.last().is_none_or(|s| !s.ends_with(']'))
                {
                    return Err(invalid());
                }
                snapshot_number(shadow, "shadow_obj=", 10)?;
                snapshot_number(results, "result_count=", 10)?;
                snapshot_number(saved, "save_words=", 10)?;
            }
            _ => return Err(invalid()),
        }
    }
    if functions == 0 {
        return Err(invalid());
    }
    if dynamic {
        return Err("Sonatina recursive spills require dynamic native frames".into());
    }
    Ok(if spills || bound > arena { bound } else { 0 })
}

fn snapshot_number(text: &str, prefix: &str, radix: u32) -> Result<u64, String> {
    text.strip_prefix(prefix)
        .filter(|text| !text.is_empty() && text.chars().all(|c| c.is_digit(radix)))
        .and_then(|text| u64::from_str_radix(text, radix).ok())
        .ok_or_else(|| "unrecognized Sonatina memory plan snapshot".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_PLAN: &str = "evm mem plan: global_dyn_base=0x80 arena_base=0x80 scratch_peak_words=0 stable_chain_peak_words=0\nevm mem plan: entry scratch_words=0 stable_words=0 stable_mode=None entry_abs_words=0 abs_words_end=0\n";

    #[test]
    fn native_memory_bound() {
        assert_eq!(memory_bound(EMPTY_PLAN), Ok(0));
        assert_eq!(
            memory_bound(&format!("{EMPTY_PLAN}  scratch_spill v7 slot=0 addr=0x0\n")),
            Ok(128)
        );
        assert_eq!(
            memory_bound(
                "evm mem plan: global_dyn_base=0xc0 arena_base=0x80 scratch_peak_words=1 stable_chain_peak_words=1\nevm mem plan: entry scratch_words=1 stable_words=1 stable_mode=StableAbs(base=0xa0) entry_abs_words=1 abs_words_end=2\n  spill v7 offset_words=0 loc=StableAbs(0) addr=0xa0\n"
            ),
            Ok(192)
        );
        snapbox::assert_data_eq!(
            memory_bound(&EMPTY_PLAN.replace("stable_mode=None", "stable_mode=DynamicFrame"))
                .unwrap_err(),
            snapbox::str!["Sonatina recursive spills require dynamic native frames"]
        );
    }

    #[test]
    fn reject_changed_memory_snapshot() {
        for snapshot in [
            EMPTY_PLAN.replace("global_dyn_base", "dynamic_base"),
            EMPTY_PLAN.replace("arena_base=0x80", "arena_base=unknown"),
            EMPTY_PLAN.replace("stable_mode=None", "stable_mode=NewMode"),
            EMPTY_PLAN.replace("global_dyn_base=0x80", "global_dyn_base=0xa0"),
            EMPTY_PLAN.replace("global_dyn_base=0x80", "global_dyn_base=0x10000000000000000"),
            EMPTY_PLAN.lines().next().unwrap().to_string(),
            format!("{EMPTY_PLAN}  new_spill v7 offset=0\n"),
        ] {
            snapbox::assert_data_eq!(
                memory_bound(&snapshot).unwrap_err(),
                snapbox::str!["unrecognized Sonatina memory plan snapshot"]
            );
        }
    }
}
