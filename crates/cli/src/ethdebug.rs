//! ETHDebug resources and programs shared by CLI and Standard JSON output.

use serde::Serialize;
use serde_json::{Map, Value};
use solar_codegen::{
    ContractArtifact,
    backend::evm::{DebugFunction, DebugFunctionExit, DebugInstruction},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::{Gcx, hir::ContractId};

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum EthdebugId {
    Number(u32),
    Text(String),
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugReference {
    id: EthdebugId,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugRange {
    offset: usize,
    length: usize,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugSourceRange {
    source: EthdebugReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<EthdebugRange>,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugFunctionInvoke {
    #[serde(skip_serializing_if = "Option::is_none")]
    identifier: Option<String>,
    declaration: EthdebugSourceRange,
    jump: bool,
    target: EthdebugInvocationTarget,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugInvocationTarget {
    pointer: EthdebugCodePointer,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugCodePointer {
    location: &'static str,
    offset: usize,
    length: usize,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugFunctionExit {}

#[derive(Clone, Debug, Serialize)]
struct EthdebugContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<EthdebugSourceRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pick: Vec<Self>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invoke: Option<EthdebugFunctionInvoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#return: Option<EthdebugFunctionExit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revert: Option<EthdebugFunctionExit>,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugOperation {
    mnemonic: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    arguments: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugInstruction {
    offset: usize,
    operation: EthdebugOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<EthdebugContext>,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugContract {
    name: String,
    definition: EthdebugSourceRange,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum EthdebugEnvironment {
    Call,
    Create,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugProgram {
    compilation: EthdebugReference,
    contract: EthdebugContract,
    environment: EthdebugEnvironment,
    instructions: Vec<EthdebugInstruction>,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugCompiler {
    name: String,
    version: String,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugSource {
    id: EthdebugId,
    path: String,
    contents: String,
    language: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugCompilation {
    id: EthdebugId,
    compiler: EthdebugCompiler,
    sources: Vec<EthdebugSource>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugResources {
    compilation: EthdebugCompilation,
    types: Map<String, Value>,
    pointers: Map<String, Value>,
}

impl EthdebugCompilation {
    pub(crate) fn id(&self) -> &str {
        let EthdebugId::Text(id) = &self.id else { unreachable!() };
        id
    }

    pub(crate) fn into_resources(self) -> EthdebugResources {
        EthdebugResources {
            compilation: self,
            types: Default::default(),
            pointers: Default::default(),
        }
    }
}

pub(crate) fn make_ethdebug_compilation(
    gcx: Gcx<'_>,
    metadata_identity: Option<alloy_primitives::B256>,
) -> EthdebugCompilation {
    let language = if gcx.sess.opts.language.is_yul() { "Yul" } else { "Solidity" };
    let sources = gcx
        .hir
        .source_ids()
        .map(|source_id| {
            let source = gcx.hir.source(source_id);
            EthdebugSource {
                id: EthdebugId::Number(source_id.index() as u32),
                path: source.file.name.display().to_string().replace('\\', "/"),
                contents: source.file.src.as_ref().clone(),
                language: language.to_owned(),
            }
        })
        .collect::<Vec<_>>();

    let version = solar_config::version::SEMVER_VERSION.to_owned();
    let mut identity = String::from("ethdebug-solar-compilation-v1");
    // A compilation ID names the complete source-to-bytecode context, not only
    // the source files. Include every code-generation setting that can change
    // instruction offsets or operations so programs cannot cross-reference a
    // different artifact accidentally.
    append_length_prefixed(&mut identity, solar_config::version::SHORT_VERSION);
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.language));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.evm_version));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.optimization));
    append_length_prefixed(&mut identity, &gcx.sess.opts.revert_strings.to_string());
    append_length_prefixed(&mut identity, &format!("{metadata_identity:?}"));
    append_length_prefixed(
        &mut identity,
        &gcx.sess.opts.optimizer_runs.map_or_else(|| "none".to_owned(), |runs| runs.to_string()),
    );
    // Lookup precedence is order-sensitive, including duplicate library names.
    // Sorting these inputs would alias compilations that link different code.
    append_length_prefixed(&mut identity, &gcx.sess.opts.import_remappings.len().to_string());
    for remapping in &gcx.sess.opts.import_remappings {
        append_length_prefixed(&mut identity, &remapping.to_string());
    }
    append_length_prefixed(&mut identity, &gcx.sess.opts.libraries.len().to_string());
    for library in &gcx.sess.opts.libraries {
        append_length_prefixed(&mut identity, &library.to_string());
    }
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.mir_pipeline));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.evm_ir_pipeline));
    append_length_prefixed(&mut identity, &format!("{:?}", gcx.sess.opts.unstable.switch_lowering));
    append_length_prefixed(
        &mut identity,
        &format!(
            "{:?}:{:?}:{:?}",
            gcx.sess.opts.unstable.switch_max_gas_code_growth,
            gcx.sess.opts.unstable.switch_max_bit_slice_gas_code_growth,
            gcx.sess.opts.unstable.codegen_all_functions,
        ),
    );
    append_length_prefixed(&mut identity, &version);
    append_length_prefixed(&mut identity, &sources.len().to_string());
    for source in &sources {
        let EthdebugId::Number(id) = source.id else { unreachable!() };
        append_length_prefixed(&mut identity, &id.to_string());
        append_length_prefixed(&mut identity, &source.path);
        append_length_prefixed(&mut identity, &source.contents);
        append_length_prefixed(&mut identity, &source.language);
    }
    let digest = alloy_primitives::keccak256(identity.as_bytes());
    let id = format!("solar-{}", alloy_primitives::hex::encode(digest.as_slice()));

    EthdebugCompilation {
        id: EthdebugId::Text(id),
        compiler: EthdebugCompiler { name: "solar".to_owned(), version },
        sources,
    }
}

fn append_length_prefixed(output: &mut String, value: &str) {
    output.push_str(&value.len().to_string());
    output.push(':');
    output.push_str(value);
}

pub(crate) fn make_ethdebug_program(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    artifact: &ContractArtifact,
    compilation_id: &str,
    deployed: bool,
) -> Option<EthdebugProgram> {
    let debug_info = if deployed {
        artifact.runtime_debug_info.as_ref()?
    } else {
        artifact.deployment_debug_info.as_ref()?
    };
    let bytecode = if deployed { artifact.runtime.as_ref() } else { artifact.deployment.as_ref() };
    let link_references = if deployed {
        &artifact.runtime_link_references
    } else {
        &artifact.deployment_link_references
    };
    let unlinked_arguments =
        link_references.iter().map(|link| link.start).collect::<FxHashSet<_>>();
    let contract = gcx.hir.contract(contract_id);
    let source_ids = gcx
        .hir
        .source_ids()
        .map(|source_id| (gcx.hir.source(source_id).file.start_pos.0, source_id.index() as u32))
        .collect::<FxHashMap<_, _>>();
    let definition_range = gcx
        .sess
        .source_map()
        .span_to_range(contract.span)
        .ok()
        .map(|range| EthdebugRange { offset: range.start, length: range.end - range.start });

    let instructions = debug_info
        .iter()
        .enumerate()
        .map(|(index, instruction)| {
            let mnemonic = solar_codegen::backend::evm::opcode_mnemonic(instruction.opcode)
                .expect("assembled opcode should have a mnemonic")
                .to_ascii_uppercase();
            let arguments = push_argument(bytecode, instruction)
                // NOTE: A library relocation is not a concrete operand. Omit it
                // until linking, instead of exposing the backend's placeholder
                // bytes as an address that can become stale after linking.
                .filter(|_| !unlinked_arguments.contains(&(instruction.offset as usize + 1)))
                .map(|argument| format!("0x{}", alloy_primitives::hex::encode(argument)))
                .into_iter()
                .collect();
            EthdebugInstruction {
                offset: instruction.offset as usize,
                operation: EthdebugOperation { mnemonic, arguments },
                context: make_ethdebug_context(
                    gcx,
                    &source_ids,
                    bytecode,
                    debug_info.get(index.wrapping_sub(1)),
                    instruction,
                ),
            }
        })
        .collect();

    Some(EthdebugProgram {
        compilation: EthdebugReference { id: EthdebugId::Text(compilation_id.to_owned()) },
        contract: EthdebugContract {
            name: contract.name.to_string(),
            definition: EthdebugSourceRange {
                source: EthdebugReference {
                    id: EthdebugId::Number(contract.source.index() as u32),
                },
                range: definition_range,
            },
        },
        environment: if deployed { EthdebugEnvironment::Call } else { EthdebugEnvironment::Create },
        instructions,
    })
}

fn push_argument<'a>(bytecode: &'a [u8], instruction: &DebugInstruction) -> Option<&'a [u8]> {
    let width = instruction.opcode.checked_sub(0x5f)? as usize;
    if !(1..=32).contains(&width) {
        return None;
    }
    let start = instruction.offset as usize + 1;
    let end = start.checked_add(width)?;
    bytecode.get(start..end)
}

fn make_ethdebug_context(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    bytecode: &[u8],
    previous: Option<&DebugInstruction>,
    instruction: &DebugInstruction,
) -> Option<EthdebugContext> {
    let mut contexts = instruction
        .source_spans
        .iter()
        .filter_map(|&span| make_ethdebug_source_range(gcx, source_ids, span))
        .map(|code| EthdebugContext {
            code: Some(code),
            pick: Vec::new(),
            invoke: None,
            r#return: None,
            revert: None,
        })
        .collect::<Vec<_>>();
    let (code, pick) = match contexts.len() {
        0 => (None, Vec::new()),
        1 => (contexts.pop().and_then(|context| context.code), Vec::new()),
        _ => (None, contexts),
    };
    let invoke = instruction.function_invoke.and_then(|function| {
        make_ethdebug_function_invoke(gcx, source_ids, bytecode, previous, function, instruction)
    });
    let (r#return, revert) = match (instruction.function_exit, instruction.opcode) {
        (Some(DebugFunctionExit::Return), 0x00 | 0x56 | 0x57 | 0xf3) => {
            (Some(EthdebugFunctionExit {}), None)
        }
        (Some(DebugFunctionExit::Revert), 0xfd) => (None, Some(EthdebugFunctionExit {})),
        _ => (None, None),
    };
    if code.is_none()
        && pick.is_empty()
        && invoke.is_none()
        && r#return.is_none()
        && revert.is_none()
    {
        None
    } else {
        Some(EthdebugContext { code, pick, invoke, r#return, revert })
    }
}

fn make_ethdebug_function_invoke(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    bytecode: &[u8],
    previous: Option<&DebugInstruction>,
    function: DebugFunction,
    instruction: &DebugInstruction,
) -> Option<EthdebugFunctionInvoke> {
    // NOTE: Constructors, entry labels, dynamic jumps, and optimized fallthroughs
    // are not statically identified internal calls. Leave their invocation
    // unknown rather than inventing a jump target or changing executable code.
    let target = crate::source_map::static_jump_target(bytecode, previous, instruction)?;
    let target = EthdebugInvocationTarget {
        pointer: EthdebugCodePointer { location: "code", offset: target, length: 1 },
    };
    Some(EthdebugFunctionInvoke {
        identifier: (function.identifier != solar_interface::sym::_anonymous)
            .then(|| function.identifier.to_string()),
        declaration: make_ethdebug_source_range(gcx, source_ids, function.declaration)?,
        jump: true,
        target,
    })
}

fn make_ethdebug_source_range(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    span: solar_interface::Span,
) -> Option<EthdebugSourceRange> {
    let source = gcx.sess.source_map().span_to_source(span).ok()?;
    let source_id = *source_ids.get(&source.file.start_pos.0)?;
    Some(EthdebugSourceRange {
        source: EthdebugReference { id: EthdebugId::Number(source_id) },
        range: Some(EthdebugRange {
            offset: source.data.start,
            length: source.data.end - source.data.start,
        }),
    })
}
