//! ETHDebug resources and programs shared by CLI and Standard JSON output.

use serde::Serialize;
use serde_json::{Map, Value};
use solar_codegen::{
    ContractArtifact,
    backend::evm::{DebugFunction, DebugFunctionExit, DebugInstruction, op},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::{Gcx, hir::ContractId};

use crate::source_map::{enters_function, function_entries};

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum EthdebugId {
    Number(u32),
    Text(String),
}

/// The version of the ethdebug/format specification the outputs follow.
const ETHDEBUG_SPECIFICATION_VERSION: &str = "0.1.0-draft.2";

#[derive(Clone, Debug, Serialize)]
struct EthdebugStamp {
    schema: &'static str,
    version: &'static str,
}

impl EthdebugStamp {
    const fn new(schema: &'static str) -> Self {
        Self { schema, version: ETHDEBUG_SPECIFICATION_VERSION }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum EthdebugReferenceType {
    Compilation,
    Source,
}

#[derive(Clone, Debug, Serialize)]
struct EthdebugReference {
    id: EthdebugId,
    r#type: EthdebugReferenceType,
}

impl EthdebugReference {
    fn source(id: u32) -> Self {
        Self { id: EthdebugId::Number(id), r#type: EthdebugReferenceType::Source }
    }

    fn compilation(id: &str) -> Self {
        Self { id: EthdebugId::Text(id.to_owned()), r#type: EthdebugReferenceType::Compilation }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<EthdebugInvocationTarget>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gather: Vec<Self>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invoke: Option<EthdebugFunctionInvoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#return: Option<EthdebugFunctionExit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revert: Option<EthdebugFunctionExit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    transform: Vec<&'static str>,
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
    ethdebug: EthdebugStamp,
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
    ethdebug: EthdebugStamp,
    types: Map<String, Value>,
    pointers: Map<String, Value>,
}

/// The global ethdebug outputs, shared by the CLI and Standard JSON.
#[derive(Debug, Default, Serialize)]
pub(crate) struct EthdebugOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resources: Option<EthdebugResources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) compilation: Option<EthdebugCompilation>,
}

impl EthdebugCompilation {
    pub(crate) fn id(&self) -> &str {
        let EthdebugId::Text(id) = &self.id else { unreachable!() };
        id
    }
}

impl EthdebugResources {
    pub(crate) fn new() -> Self {
        Self {
            ethdebug: EthdebugStamp::new("ethdebug/format/info/resources"),
            types: Default::default(),
            pointers: Default::default(),
        }
    }
}

#[tracing::instrument(name = "ethdebug_compilation", level = "debug", skip_all)]
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
                path: source.file.name.display().to_string(),
                contents: source.file.src.as_ref().clone(),
                language: language.to_owned(),
            }
        })
        .collect::<Vec<_>>();

    let version = solar_config::version::solar_version().to_owned();
    let mut identity = String::from("ethdebug-solar-compilation-v1");
    // A compilation ID names the complete source-to-bytecode context, not only
    // the source files. Include every code-generation setting that can change
    // instruction offsets or operations so programs cannot cross-reference a
    // different artifact accidentally.
    append_length_prefixed(&mut identity, solar_config::version::solc_compat_version());
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
    append_length_prefixed(
        &mut identity,
        &format!(
            "{:?}:{:?}",
            gcx.sess.opts.unstable.tune, gcx.sess.opts.unstable.pack_wide_jump_tables,
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
    let id = format!("solar-{}", alloy_primitives::hex::display(digest));

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

    let function_entries = function_entries(debug_info, bytecode);
    let mut previous = None;
    let instructions = debug_info
        .instructions(bytecode)
        .map(|instruction| {
            let mnemonic = op::mnemonic(instruction.opcode)
                .expect("assembled opcode should have a mnemonic")
                .to_ascii_uppercase();
            let arguments = push_argument(bytecode, &instruction)
                // NOTE: A library relocation is not a concrete operand. Omit it
                // until linking, instead of exposing the backend's placeholder
                // bytes as an address that can become stale after linking.
                .filter(|_| !unlinked_arguments.contains(&(instruction.offset as usize + 1)))
                .map(|argument| format!("0x{}", alloy_primitives::hex::display(argument)))
                .into_iter()
                .collect();
            let context = make_ethdebug_context(
                gcx,
                &source_ids,
                bytecode,
                &function_entries,
                previous.as_ref(),
                &instruction,
            );
            previous = Some(instruction);
            EthdebugInstruction {
                offset: instruction.offset as usize,
                operation: EthdebugOperation { mnemonic, arguments },
                context,
            }
        })
        .collect();

    Some(EthdebugProgram {
        ethdebug: EthdebugStamp::new("ethdebug/format/program"),
        compilation: EthdebugReference::compilation(compilation_id),
        contract: EthdebugContract {
            name: contract.name.to_string(),
            definition: EthdebugSourceRange {
                source: EthdebugReference::source(contract.source.index() as u32),
                range: definition_range,
            },
        },
        environment: if deployed { EthdebugEnvironment::Call } else { EthdebugEnvironment::Create },
        instructions,
    })
}

fn push_argument<'a>(bytecode: &'a [u8], instruction: &DebugInstruction<'_>) -> Option<&'a [u8]> {
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
    function_entries: &FxHashSet<u32>,
    previous: Option<&DebugInstruction<'_>>,
    instruction: &DebugInstruction<'_>,
) -> Option<EthdebugContext> {
    let location = instruction.location;
    let mut contexts = location
        .source_spans
        .iter()
        .filter_map(|&span| make_ethdebug_source_range(gcx, source_ids, span))
        .map(|code| EthdebugContext {
            code: Some(code),
            pick: Vec::new(),
            gather: Vec::new(),
            invoke: None,
            r#return: None,
            revert: None,
            transform: Vec::new(),
        })
        .collect::<Vec<_>>();
    // Several spans are alternatives, one of which holds, unless an optimization folded the work
    // of several instructions into this one, so that every span holds.
    let (code, pick, gather) = match contexts.len() {
        0 => (None, Vec::new(), Vec::new()),
        1 => (contexts.pop().and_then(|context| context.code), Vec::new(), Vec::new()),
        _ if location.all_source_spans => (None, Vec::new(), contexts),
        _ => (None, contexts, Vec::new()),
    };
    let invoke = location
        .function_invoke
        .and_then(|function| make_ethdebug_function_invoke(gcx, source_ids, function, instruction));
    let (r#return, revert) = match (location.function_exit, instruction.opcode) {
        (Some(DebugFunctionExit::Return), 0x00 | 0x56 | 0x57 | 0xf3) => {
            (Some(EthdebugFunctionExit {}), None)
        }
        (Some(DebugFunctionExit::Revert), 0xfd) => (None, Some(EthdebugFunctionExit {})),
        _ => (None, None),
    };
    // A tail call leaves the caller and enters the callee with one `JUMP`, so that jump carries the
    // caller's `return` and `transform: ["tailcall"]`. The callee's entry `JUMPDEST` carries its
    // `invoke`, as for any call, so the jump needs none.
    let mut transform = Vec::new();
    if r#return.is_some()
        && instruction.opcode == op::JUMP
        && enters_function(bytecode, previous, instruction, function_entries)
    {
        transform.push("tailcall");
    }
    if code.is_none()
        && pick.is_empty()
        && gather.is_empty()
        && invoke.is_none()
        && r#return.is_none()
        && revert.is_none()
    {
        None
    } else {
        Some(EthdebugContext { code, pick, gather, invoke, r#return, revert, transform })
    }
}

fn make_ethdebug_function_invoke(
    gcx: Gcx<'_>,
    source_ids: &FxHashMap<u32, u32>,
    function: DebugFunction,
    instruction: &DebugInstruction<'_>,
) -> Option<EthdebugFunctionInvoke> {
    // The event marks the first instruction of the callee: its entry `JUMPDEST`, which is where
    // the ethdebug format places an internal call's `invoke`.
    // NOTE: Code that nothing jumps into, such as an inlined body or a constructor without
    // helpers, starts at another instruction, and there is no jump target to point at.
    let target = (instruction.opcode == op::JUMPDEST).then_some(EthdebugInvocationTarget {
        pointer: EthdebugCodePointer {
            location: "code",
            offset: instruction.offset as usize,
            length: 1,
        },
    });
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
        source: EthdebugReference::source(source_id),
        range: Some(EthdebugRange {
            offset: source.data.start,
            length: source.data.end - source.data.start,
        }),
    })
}
