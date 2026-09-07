//! Standard JSON data structures, serialization, selection parsing, and statistics.

use crate::bytecode::MaybeHexBytecode;
use alloy_primitives::Address;
use indexmap::IndexMap;
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};
use serde_json::{Map, Value};
use solar_config::RevertStrings;
use solar_data_structures::map::FxBuildHasher;
use solar_interface::diagnostics::SolcDiagnostic;
use solar_sema::output::{Documentation, StorageLayoutOutput};
use std::{
    borrow::{Borrow, Cow},
    fmt,
    ops::Deref,
};

pub(super) type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

/// Result returned by a Standard JSON read callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadCallbackResult {
    /// The requested data was found.
    Success(String),
    /// The callback handled the request and returned an error.
    Error(String),
    /// The callback does not support this request kind.
    Unsupported,
}

/// Callback used by Standard JSON compilation to retrieve extra input.
pub trait StandardJsonReadCallback: Send + Sync + 'static {
    /// Reads data for `kind`.
    fn read(&self, kind: &str, data: &str) -> ReadCallbackResult;
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompilerInput<'a> {
    #[serde(default = "default_language")]
    pub(super) language: CowStr<'a>,
    #[serde(borrow, default)]
    pub(super) sources: FxIndexMap<CowStr<'a>, SourceInput<'a>>,
    #[serde(borrow, default)]
    pub(super) settings: Settings<'a>,
    //
    // Not supported.
    // #[serde(borrow, default)]
    // auxiliary_input: Option<CowValue<'a>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceInput<'a> {
    #[serde(borrow)]
    pub(super) content: Option<CowStr<'a>>,
    #[serde(borrow, default)]
    pub(super) urls: Vec<CowStr<'a>>,
    //
    // Not supported.
    // #[serde(borrow)]
    // keccak256: Option<CowValue<'a>>,
    // #[serde(borrow)]
    // ast: Option<CowValue<'a>>,
    // #[serde(borrow)]
    // assembly_json: Option<CowValue<'a>>,
}

// The supported subset of solc's Standard JSON `settings` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings<'a> {
    #[serde(borrow, default)]
    pub(super) remappings: Vec<CowStr<'a>>,
    #[serde(borrow, default)]
    pub(super) output_selection: OutputSelection<'a>,
    #[serde(borrow)]
    pub(super) stop_after: Option<CowStr<'a>>,
    #[serde(borrow)]
    pub(super) evm_version: Option<CowStr<'a>>,
    #[serde(default)]
    pub(super) optimizer: Option<Optimizer>,
    #[serde(default)]
    pub(super) metadata: MetadataSettings,
    #[serde(borrow, default)]
    pub(super) libraries: Libraries<'a>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub(super) debug: Option<DebugSettings>,
    //
    // Not supported.
    // #[serde(borrow, default)]
    // experimental: Option<CowValue<'a>>,
    // #[serde(borrow, default)]
    // model_checker: Option<CowValue<'a>>,
    // #[serde(default)]
    // via_ir: Option<bool>,
    // #[serde(default)]
    // via_ssa_cfg: Option<bool>,
}

/// The solc Standard JSON `settings.debug` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DebugSettings {
    /// Revert reason string handling.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub(super) revert_strings: Option<RevertStrings>,
    /// Debug info components to include in IR output.
    ///
    /// We do not emit Yul IR, so only the selection rules are enforced: `snippet` requires
    /// `location`, and an explicit selection must include `ethdebug` to request ethdebug output.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub(super) debug_info: Option<Vec<DebugInfoComponent>>,
}

impl DebugSettings {
    /// Returns `true` if `component` is selected.
    ///
    /// Like solc, `*` selects exactly the non-experimental components (`location`, `snippet`,
    /// and `ast-id`) regardless of what else is listed. Returns `false` when `debugInfo` is
    /// absent; callers apply solc's defaults themselves.
    pub(super) fn selects_debug_info(&self, component: DebugInfoComponent) -> bool {
        let Some(components) = self.debug_info.as_deref() else { return false };
        if components.contains(&DebugInfoComponent::All) {
            return component.is_selected_by_wildcard();
        }
        components.contains(&component)
    }
}

/// A solc `DebugInfoSelection` component name.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum DebugInfoComponent {
    /// Source locations, `@src` in Yul IR.
    Location,
    /// Source snippets next to locations.
    Snippet,
    /// AST node IDs, `@ast-id` in Yul IR.
    AstId,
    /// Ethdebug annotations.
    Ethdebug,
    /// Every non-experimental component.
    #[serde(rename = "*")]
    All,
}

impl DebugInfoComponent {
    /// Returns `true` if `*` selects this component; experimental `ethdebug` is excluded.
    const fn is_selected_by_wildcard(self) -> bool {
        matches!(self, Self::Location | Self::Snippet | Self::AstId)
    }
}

/// The solc Standard JSON `settings.metadata` object.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MetadataSettings {
    #[serde(default = "default_true")]
    #[serde(rename = "appendCBOR")]
    pub(super) append_cbor: bool,
    #[serde(default)]
    pub(super) use_literal_content: bool,
    #[serde(default)]
    pub(super) bytecode_hash: MetadataHashSetting,
}

impl Default for MetadataSettings {
    fn default() -> Self {
        Self { append_cbor: true, use_literal_content: false, bytecode_hash: Default::default() }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MetadataHashSetting {
    pub(super) value: MetadataHash,
    pub(super) is_explicit: bool,
}

impl<'de> Deserialize<'de> for MetadataHashSetting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        MetadataHash::deserialize(deserializer).map(|value| Self { value, is_explicit: true })
    }
}

/// Hash embedded in the bytecode metadata trailer.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum MetadataHash {
    #[default]
    Ipfs,
    Bzzr1,
    None,
}

const fn default_true() -> bool {
    true
}

/// Deserializes an optional field that may be omitted but not `null`, like solc's presence
/// checks: `#[serde(default)]` supplies `None` for an absent field, and a present field must
/// hold a `T`.
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// The supported subset of solc's Standard JSON `settings.optimizer` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Optimizer {
    /// Whether the optimizer is enabled.
    #[serde(default)]
    pub(super) enabled: bool,
    /// Expected executions per deployment for optimizer tradeoffs.
    #[serde(default)]
    pub(super) runs: Option<u64>,
}

pub(super) fn optimizer_settings(optimizer: Option<&Optimizer>) -> (bool, u64) {
    (
        optimizer.is_some_and(|optimizer| optimizer.enabled),
        optimizer.and_then(|optimizer| optimizer.runs).unwrap_or(200),
    )
}

/// The solc Standard JSON `settings.libraries` object.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Libraries<'a>(
    #[serde(borrow)] pub(super) FxIndexMap<CowStr<'a>, FxIndexMap<CowStr<'a>, Address>>,
);

impl Libraries<'_> {
    pub(super) fn len(&self) -> usize {
        self.0.values().map(FxIndexMap::len).sum()
    }
}

#[derive(Debug, Default, Serialize)]
pub(super) struct CompilerOutput<'gcx> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) errors: Vec<SolcDiagnostic<'gcx>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub(super) sources: FxIndexMap<String, SourceOutput>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub(super) contracts: FxIndexMap<String, FxIndexMap<String, ContractOutput<'gcx>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ethdebug: Option<EthdebugOutput>,
}

#[derive(Debug, Serialize)]
pub(super) struct SourceOutput {
    pub(super) id: u32,
    //
    // Not supported.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ast: Option<CowValue<'a>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContractOutput<'gcx> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) abi: Option<&'gcx [alloy_json_abi::AbiItem<'gcx>]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) userdoc: Option<&'gcx Documentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) devdoc: Option<&'gcx Documentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) storage_layout: Option<StorageLayoutOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) transient_storage_layout: Option<StorageLayoutOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) evm: Option<EvmOutput>,
    //
    // Not supported.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir: Option<CowValue<'a>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_ast: Option<CowValue<'a>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized: Option<CowValue<'a>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ir_optimized_ast: Option<CowValue<'a>>,
    // #[serde(rename = "yulCFGJson", skip_serializing_if = "Option::is_none")]
    // yul_cfg_json: Option<CowValue<'a>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvmOutput {
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub(super) method_identifiers: FxIndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bytecode: Option<BytecodeOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) deployed_bytecode: Option<BytecodeOutput>,
    //
    // Not supported.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // assembly: Option<CowValue<'a>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // legacy_assembly: Option<CowValue<'a>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // gas_estimates: Option<CowValue<'a>>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BytecodeOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) object: Option<MaybeHexBytecode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ethdebug: Option<EthdebugProgram>,
    // Function debug data is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // function_debug_data: Option<CowValue<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) opcodes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source_map: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) link_references: Option<FxIndexMap<String, FxIndexMap<String, Vec<OffsetLength>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) immutable_references: Option<FxIndexMap<String, Vec<OffsetLength>>>,
    //
    // Not supported.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // generated_sources: Option<CowValue<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub(super) enum EthdebugId {
    Number(u32),
    Text(String),
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugReference {
    pub(super) id: EthdebugId,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugRange {
    pub(super) offset: usize,
    pub(super) length: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugSourceRange {
    pub(super) source: EthdebugReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) range: Option<EthdebugRange>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugFunctionInvoke {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) identifier: Option<String>,
    pub(super) declaration: EthdebugSourceRange,
    pub(super) jump: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) target: Option<EthdebugInvocationTarget>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugInvocationTarget {
    pub(super) pointer: EthdebugCodePointer,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugCodePointer {
    pub(super) location: &'static str,
    pub(super) offset: usize,
    pub(super) length: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugFunctionExit {}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) code: Option<EthdebugSourceRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) pick: Vec<Self>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) invoke: Option<EthdebugFunctionInvoke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) r#return: Option<EthdebugFunctionExit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) revert: Option<EthdebugFunctionExit>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugOperation {
    pub(super) mnemonic: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) arguments: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugInstruction {
    pub(super) offset: usize,
    pub(super) operation: EthdebugOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) context: Option<EthdebugContext>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugContract {
    pub(super) name: String,
    pub(super) definition: EthdebugSourceRange,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum EthdebugEnvironment {
    Call,
    Create,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugProgram {
    pub(super) compilation: EthdebugReference,
    pub(super) contract: EthdebugContract,
    pub(super) environment: EthdebugEnvironment,
    pub(super) instructions: Vec<EthdebugInstruction>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugCompiler {
    pub(super) name: String,
    pub(super) version: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EthdebugSource {
    pub(super) id: EthdebugId,
    pub(super) path: String,
    pub(super) contents: String,
    pub(super) language: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugCompilation {
    pub(super) id: EthdebugId,
    pub(super) compiler: EthdebugCompiler,
    pub(super) sources: Vec<EthdebugSource>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EthdebugResources {
    pub(super) compilation: EthdebugCompilation,
    pub(super) types: Map<String, Value>,
    pub(super) pointers: Map<String, Value>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct EthdebugOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) resources: Option<EthdebugResources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) compilation: Option<EthdebugCompilation>,
}

#[derive(Debug, Serialize)]
pub(super) struct OffsetLength {
    pub(super) start: usize,
    pub(super) length: usize,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct OutputSelection<'a>(
    #[serde(borrow)] FxIndexMap<CowStr<'a>, FxIndexMap<CowStr<'a>, OutputSelectionFlags>>,
);

impl<'a> OutputSelection<'a> {
    pub(super) fn all(&self) -> OutputSelectionFlags {
        self.source("*")
    }

    /// Returns every flag selected for any source or contract, including the wildcards.
    pub(super) fn union(&self) -> OutputSelectionFlags {
        self.0
            .values()
            .flat_map(FxIndexMap::values)
            .fold(OutputSelectionFlags::empty(), |acc, &f| acc | f)
    }

    pub(super) fn global(&self) -> OutputSelectionFlags {
        self.0.get("*").and_then(|contracts| contracts.get("*")).copied().unwrap_or_default()
            & OutputSelectionFlags::GLOBAL
    }

    pub(super) fn source(&self, source: &str) -> OutputSelectionFlags {
        self.contract(source, "*")
    }

    pub(super) fn contract(&self, source: &str, contract: &str) -> OutputSelectionFlags {
        fn contract_flags(
            contracts: &FxIndexMap<CowStr<'_>, OutputSelectionFlags>,
            contract: &str,
        ) -> OutputSelectionFlags {
            contracts.get(contract).copied().unwrap_or_default()
        }

        let mut flags = OutputSelectionFlags::default();
        if let Some(c) = self.0.get(source) {
            flags |= contract_flags(c, contract);
            if contract != "*" {
                flags |= contract_flags(c, "*");
            }
        }

        if source != "*"
            && let Some(c) = self.0.get("*")
        {
            flags |= contract_flags(c, contract);
            if contract != "*" {
                flags |= contract_flags(c, "*");
            }
        }

        flags & OutputSelectionFlags::CONTRACT
    }

    pub(super) fn requests_metadata(&self) -> bool {
        self.0.values().any(|contracts| {
            contracts.values().any(|flags| flags.contains(OutputSelectionFlags::METADATA))
        })
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct OutputSelectionFlags: u64 {
        const AST = 1 << 0;
        const ABI = 1 << 1;
        const METADATA = 1 << 2;
        const USERDOC = 1 << 3;
        const DEVDOC = 1 << 4;
        const STORAGE_LAYOUT = 1 << 5;
        const TRANSIENT_STORAGE_LAYOUT = 1 << 6;
        const IR = 1 << 7;
        const IR_AST = 1 << 8;
        const IR_OPTIMIZED = 1 << 9;
        const IR_OPTIMIZED_AST = 1 << 10;
        const YUL_CFG_JSON = 1 << 11;
        const ASSEMBLY = 1 << 12;
        const LEGACY_ASSEMBLY = 1 << 13;
        const METHOD_IDENTIFIERS = 1 << 14;
        const GAS_ESTIMATES = 1 << 15;
        const BYTECODE_OBJECT = 1 << 16;
        const BYTECODE_OPCODES = 1 << 17;
        const BYTECODE_SOURCE_MAP = 1 << 18;
        const BYTECODE_FUNCTION_DEBUG_DATA = 1 << 19;
        const BYTECODE_GENERATED_SOURCES = 1 << 20;
        const BYTECODE_LINK_REFERENCES = 1 << 21;
        const BYTECODE_ETHDEBUG = 1 << 22;
        const DEPLOYED_BYTECODE_OBJECT = 1 << 23;
        const DEPLOYED_BYTECODE_OPCODES = 1 << 24;
        const DEPLOYED_BYTECODE_SOURCE_MAP = 1 << 25;
        const DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA = 1 << 26;
        const DEPLOYED_BYTECODE_GENERATED_SOURCES = 1 << 27;
        const DEPLOYED_BYTECODE_LINK_REFERENCES = 1 << 28;
        const DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES = 1 << 29;
        const DEPLOYED_BYTECODE_ETHDEBUG = 1 << 30;
        const ETHDEBUG_RESOURCES = 1 << 31;
        const ETHDEBUG_COMPILATION = 1 << 32;

        const YUL = Self::IR.bits()
            | Self::IR_AST.bits()
            | Self::IR_OPTIMIZED.bits()
            | Self::IR_OPTIMIZED_AST.bits()
            | Self::YUL_CFG_JSON.bits();
        const BYTECODE = Self::BYTECODE_OBJECT.bits()
            | Self::BYTECODE_OPCODES.bits()
            | Self::BYTECODE_SOURCE_MAP.bits()
            | Self::BYTECODE_FUNCTION_DEBUG_DATA.bits()
            | Self::BYTECODE_GENERATED_SOURCES.bits()
            | Self::BYTECODE_LINK_REFERENCES.bits();
        const DEPLOYED_BYTECODE = Self::DEPLOYED_BYTECODE_OBJECT.bits()
            | Self::DEPLOYED_BYTECODE_OPCODES.bits()
            | Self::DEPLOYED_BYTECODE_SOURCE_MAP.bits()
            | Self::DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA.bits()
            | Self::DEPLOYED_BYTECODE_GENERATED_SOURCES.bits()
            | Self::DEPLOYED_BYTECODE_LINK_REFERENCES.bits()
            | Self::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES.bits();
        const EVM = Self::ASSEMBLY.bits()
            | Self::LEGACY_ASSEMBLY.bits()
            | Self::METHOD_IDENTIFIERS.bits()
            | Self::GAS_ESTIMATES.bits()
            | Self::BYTECODE.bits()
            | Self::DEPLOYED_BYTECODE.bits();
        const ETHDEBUG = Self::BYTECODE_ETHDEBUG.bits()
            | Self::DEPLOYED_BYTECODE_ETHDEBUG.bits()
            | Self::ETHDEBUG_RESOURCES.bits()
            | Self::ETHDEBUG_COMPILATION.bits();
        const SOURCE = Self::AST.bits();
        const CONTRACT = Self::ABI.bits()
            | Self::METADATA.bits()
            | Self::USERDOC.bits()
            | Self::DEVDOC.bits()
            | Self::STORAGE_LAYOUT.bits()
            | Self::TRANSIENT_STORAGE_LAYOUT.bits()
            | Self::YUL.bits()
            | Self::EVM.bits()
            | Self::BYTECODE_SOURCE_MAP.bits()
            | Self::DEPLOYED_BYTECODE_SOURCE_MAP.bits()
            | Self::BYTECODE_ETHDEBUG.bits()
            | Self::DEPLOYED_BYTECODE_ETHDEBUG.bits();
        const GLOBAL = Self::ETHDEBUG_RESOURCES.bits() | Self::ETHDEBUG_COMPILATION.bits();
        /// Outputs selected by `*` in Solidity mode.
        ///
        /// Unlike `Self::all()`, this excludes experimental Yul IR and exact-only ethdebug outputs.
        const WILDCARD = Self::SOURCE.bits()
            | Self::ABI.bits()
            | Self::METADATA.bits()
            | Self::USERDOC.bits()
            | Self::DEVDOC.bits()
            | Self::STORAGE_LAYOUT.bits()
            | Self::TRANSIENT_STORAGE_LAYOUT.bits()
            | Self::EVM.bits();
    }
}

impl OutputSelectionFlags {
    fn from_key(key: &str) -> Self {
        match key {
            "*" => Self::WILDCARD,
            "ast" => Self::AST,
            "abi" => Self::ABI,
            "metadata" => Self::METADATA,
            "userdoc" => Self::USERDOC,
            "devdoc" => Self::DEVDOC,
            "storageLayout" => Self::STORAGE_LAYOUT,
            "transientStorageLayout" => Self::TRANSIENT_STORAGE_LAYOUT,
            "ir" => Self::IR,
            "irAst" => Self::IR_AST,
            "irOptimized" => Self::IR_OPTIMIZED,
            "irOptimizedAst" => Self::IR_OPTIMIZED_AST,
            "yulCFGJson" => Self::YUL_CFG_JSON,
            "evm" => Self::EVM,
            "evm.assembly" => Self::ASSEMBLY,
            "evm.legacyAssembly" => Self::LEGACY_ASSEMBLY,
            "evm.methodIdentifiers" => Self::METHOD_IDENTIFIERS,
            "evm.gasEstimates" => Self::GAS_ESTIMATES,
            "evm.bytecode" => Self::BYTECODE,
            "evm.bytecode.object" => Self::BYTECODE_OBJECT,
            "evm.bytecode.opcodes" => Self::BYTECODE_OPCODES,
            "evm.bytecode.sourceMap" => Self::BYTECODE_SOURCE_MAP,
            "evm.bytecode.functionDebugData" => Self::BYTECODE_FUNCTION_DEBUG_DATA,
            "evm.bytecode.generatedSources" => Self::BYTECODE_GENERATED_SOURCES,
            "evm.bytecode.linkReferences" => Self::BYTECODE_LINK_REFERENCES,
            "evm.bytecode.ethdebug" => Self::BYTECODE_ETHDEBUG,
            "evm.deployedBytecode" => Self::DEPLOYED_BYTECODE,
            "evm.deployedBytecode.object" => Self::DEPLOYED_BYTECODE_OBJECT,
            "evm.deployedBytecode.opcodes" => Self::DEPLOYED_BYTECODE_OPCODES,
            "evm.deployedBytecode.sourceMap" => Self::DEPLOYED_BYTECODE_SOURCE_MAP,
            "evm.deployedBytecode.functionDebugData" => Self::DEPLOYED_BYTECODE_FUNCTION_DEBUG_DATA,
            "evm.deployedBytecode.generatedSources" => Self::DEPLOYED_BYTECODE_GENERATED_SOURCES,
            "evm.deployedBytecode.linkReferences" => Self::DEPLOYED_BYTECODE_LINK_REFERENCES,
            "evm.deployedBytecode.immutableReferences" => {
                Self::DEPLOYED_BYTECODE_IMMUTABLE_REFERENCES
            }
            "evm.deployedBytecode.ethdebug" => Self::DEPLOYED_BYTECODE_ETHDEBUG,
            "ethdebug.resources" => Self::ETHDEBUG_RESOURCES,
            "ethdebug.compilation" => Self::ETHDEBUG_COMPILATION,
            _ => Self::empty(),
        }
    }
}

impl<'de> Deserialize<'de> for OutputSelectionFlags {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OutputSelectionFlagsVisitor;

        impl<'de> Visitor<'de> for OutputSelectionFlagsVisitor {
            type Value = OutputSelectionFlags;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an array of output selection strings")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut flags = OutputSelectionFlags::empty();
                while let Some(key) = seq.next_element::<CowStr<'de>>()? {
                    flags |= OutputSelectionFlags::from_key(&key);
                    if flags.is_all() {
                        while seq.next_element::<CowStr<'de>>()?.is_some() {}
                        break;
                    }
                }
                Ok(flags)
            }
        }

        deserializer.deserialize_seq(OutputSelectionFlagsVisitor)
    }
}

/// JSON string wrapper that borrows from the standard-json input when possible.
///
/// Serde's generic `Cow<'de, str>` implementation deserializes through the
/// owned representation, so direct `Cow<'de, str>` fields allocate even when
/// the JSON backend can provide `visit_borrowed_str`. `#[serde(borrow)]` on the
/// containing fields is still needed to thread the input lifetime to this type,
/// and this visitor is what selects `Cow::Borrowed` for unescaped strings and
/// `Cow::Owned` when the deserializer has to materialize an escaped string.
///
/// See <https://github.com/serde-rs/serde/issues/1852> and
/// <https://github.com/serde-rs/serde/issues/914>.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct CowStr<'a>(Cow<'a, str>);

impl CowStr<'_> {
    fn as_cow(&self) -> &Cow<'_, str> {
        &self.0
    }
}

impl AsRef<str> for CowStr<'_> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for CowStr<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for CowStr<'_> {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CowStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CowStr<'_>> for String {
    fn from(value: CowStr<'_>) -> Self {
        value.0.into_owned()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for CowStr<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CowStrVisitor;

        impl<'de> Visitor<'de> for CowStrVisitor {
            type Value = CowStr<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Borrowed(value)))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Owned(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CowStr(Cow::Owned(value)))
            }
        }

        deserializer.deserialize_str(CowStrVisitor)
    }
}

fn default_language<'a>() -> CowStr<'a> {
    CowStr(Cow::Borrowed("Solidity"))
}

#[derive(Default)]
struct JsonTreeStats {
    nodes: usize,
    objects: usize,
    arrays: usize,
    strings: usize,
    numbers: usize,
    bools: usize,
    nulls: usize,
    object_entries: usize,
    array_elements: usize,
    object_key_bytes: usize,
    string_bytes: usize,
}

impl JsonTreeStats {
    fn add_value(&mut self, value: &Value) {
        self.nodes += 1;
        match value {
            Value::Null => self.nulls += 1,
            Value::Bool(_) => self.bools += 1,
            Value::Number(_) => self.numbers += 1,
            Value::String(value) => {
                self.strings += 1;
                self.string_bytes += value.len();
            }
            Value::Array(values) => {
                self.arrays += 1;
                self.array_elements += values.len();
                for value in values {
                    self.add_value(value);
                }
            }
            Value::Object(values) => self.add_object(values),
        }
    }

    fn add_object(&mut self, values: &Map<String, Value>) {
        self.objects += 1;
        self.object_entries += values.len();
        for (key, value) in values {
            self.object_key_bytes += key.len();
            self.add_value(value);
        }
    }
}

#[derive(Default)]
struct InputCowStats {
    borrowed: usize,
    borrowed_bytes: usize,
    owned: usize,
    owned_bytes: usize,
}

impl InputCowStats {
    fn add(&mut self, value: &CowStr<'_>) {
        match value.as_cow() {
            Cow::Borrowed(value) => {
                self.borrowed += 1;
                self.borrowed_bytes += value.len();
            }
            Cow::Owned(value) => {
                self.owned += 1;
                self.owned_bytes += value.len();
            }
        }
    }
}

pub(super) fn print_standard_json_stats(raw_input: &str, input: &CompilerInput<'_>) {
    let mut tree = JsonTreeStats::default();
    match serde_json::from_str::<Value>(raw_input) {
        Ok(value) => tree.add_value(&value),
        Err(error) => {
            eprintln!("standard-json-stats: failed to parse JSON tree: {error}");
            return;
        }
    }

    let mut cows = InputCowStats::default();
    count_input_cows(input, &mut cows);

    let source_content_count =
        input.sources.values().filter(|source| source.content.is_some()).count();
    let source_content_bytes = input
        .sources
        .values()
        .filter_map(|source| source.content.as_ref())
        .map(|content| content.len())
        .sum::<usize>();
    let source_url_count = input.sources.values().map(|source| source.urls.len()).sum::<usize>();

    eprintln!(
        "standard-json-stats: input_bytes={} nodes={} objects={} arrays={} strings={} numbers={} bools={} nulls={} object_entries={} array_elements={} object_key_bytes={} string_bytes={}",
        raw_input.len(),
        tree.nodes,
        tree.objects,
        tree.arrays,
        tree.strings,
        tree.numbers,
        tree.bools,
        tree.nulls,
        tree.object_entries,
        tree.array_elements,
        tree.object_key_bytes,
        tree.string_bytes,
    );
    eprintln!(
        "standard-json-stats: sources={} source_content_count={} source_content_bytes={} source_url_count={} remappings={} output_selection_sources={}",
        input.sources.len(),
        source_content_count,
        source_content_bytes,
        source_url_count,
        input.settings.remappings.len(),
        input.settings.output_selection.0.len(),
    );
    eprintln!(
        "standard-json-stats: cow_borrowed={} cow_borrowed_bytes={} cow_owned={} cow_owned_bytes={}",
        cows.borrowed, cows.borrowed_bytes, cows.owned, cows.owned_bytes,
    );
}

fn count_input_cows(input: &CompilerInput<'_>, stats: &mut InputCowStats) {
    stats.add(&input.language);
    for (name, source) in &input.sources {
        stats.add(name);
        if let Some(content) = &source.content {
            stats.add(content);
        }
        for url in &source.urls {
            stats.add(url);
        }
    }
    for remapping in &input.settings.remappings {
        stats.add(remapping);
    }
    if let Some(stop_after) = &input.settings.stop_after {
        stats.add(stop_after);
    }
    if let Some(evm_version) = &input.settings.evm_version {
        stats.add(evm_version);
    }
    for (source, libraries) in &input.settings.libraries.0 {
        stats.add(source);
        for library in libraries.keys() {
            stats.add(library);
        }
    }
    for (source, contracts) in &input.settings.output_selection.0 {
        stats.add(source);
        for contract in contracts.keys() {
            stats.add(contract);
        }
    }
}

impl ContractOutput<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.abi.is_none()
            && self.metadata.is_none()
            && self.userdoc.is_none()
            && self.devdoc.is_none()
            && self.storage_layout.is_none()
            && self.transient_storage_layout.is_none()
            && self.evm.is_none()
    }
}

impl EvmOutput {
    pub(super) fn is_empty(&self) -> bool {
        self.method_identifiers.is_empty()
            && self.bytecode.is_none()
            && self.deployed_bytecode.is_none()
    }
}

pub(super) fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && ch == '/' {
                        break;
                    }
                    prev = ch;
                }
            }
            _ => out.push(ch),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_hash_rejects_null() {
        assert!(serde_json::from_str::<MetadataSettings>(r#"{"bytecodeHash":null}"#).is_err());
    }

    #[test]
    fn metadata_hash_tracks_presence() {
        let omitted = serde_json::from_str::<MetadataSettings>("{}").unwrap();
        assert_eq!(omitted.bytecode_hash.value, MetadataHash::Ipfs);
        assert!(!omitted.bytecode_hash.is_explicit);

        let explicit =
            serde_json::from_str::<MetadataSettings>(r#"{"bytecodeHash":"ipfs"}"#).unwrap();
        assert_eq!(explicit.bytecode_hash.value, MetadataHash::Ipfs);
        assert!(explicit.bytecode_hash.is_explicit);
    }

    #[test]
    fn debug_settings_parse_solc_names() {
        assert!(serde_json::from_str::<DebugSettings>(r#"{"verbose":true}"#).is_err());
        assert!(serde_json::from_str::<DebugSettings>(r#"{"revertStrings":null}"#).is_err());
        assert!(serde_json::from_str::<DebugSettings>(r#"{"debugInfo":null}"#).is_err());
        assert!(serde_json::from_str::<Settings<'_>>(r#"{"debug":null}"#).is_err());
        assert!(serde_json::from_str::<Settings<'_>>(r#"{"debug":{}}"#).is_ok());
        assert!(serde_json::from_str::<DebugSettings>(r#"{"revertStrings":"Strip"}"#).is_err());
        assert!(serde_json::from_str::<DebugSettings>(r#"{"debugInfo":["source"]}"#).is_err());
        let debug = serde_json::from_str::<DebugSettings>(
            r#"{"revertStrings":"verboseDebug","debugInfo":["location","ast-id","*"]}"#,
        )
        .unwrap();
        assert_eq!(debug.revert_strings, Some(RevertStrings::VerboseDebug));
        assert_eq!(
            debug.debug_info.as_deref(),
            Some(
                &[DebugInfoComponent::Location, DebugInfoComponent::AstId, DebugInfoComponent::All]
                    [..]
            )
        );
        assert!(debug.selects_debug_info(DebugInfoComponent::Snippet));
        assert!(!debug.selects_debug_info(DebugInfoComponent::Ethdebug));
        assert!(!DebugSettings::default().selects_debug_info(DebugInfoComponent::Ethdebug));
        let explicit =
            serde_json::from_str::<DebugSettings>(r#"{"debugInfo":["ethdebug"]}"#).unwrap();
        assert!(explicit.selects_debug_info(DebugInfoComponent::Ethdebug));
        assert!(!explicit.selects_debug_info(DebugInfoComponent::Location));
    }

    #[test]
    fn optimizer_rejects_unsupported_details() {
        assert!(serde_json::from_str::<Optimizer>(r#"{"details":{"peephole":false}}"#).is_err());
    }

    fn selection_flags(input: &str) -> OutputSelectionFlags {
        serde_json::from_str(input).unwrap()
    }

    #[test]
    fn output_selection_exact_keys() {
        let flags = selection_flags(
            r#"[
                "ast",
                "abi",
                "metadata",
                "userdoc",
                "devdoc",
                "storageLayout",
                "transientStorageLayout",
                "ir",
                "irAst",
                "irOptimized",
                "irOptimizedAst",
                "yulCFGJson",
                "evm.assembly",
                "evm.legacyAssembly",
                "evm.methodIdentifiers",
                "evm.gasEstimates",
                "evm.bytecode.object",
                "evm.bytecode.opcodes",
                "evm.bytecode.sourceMap",
                "evm.bytecode.functionDebugData",
                "evm.bytecode.generatedSources",
                "evm.bytecode.linkReferences",
                "evm.bytecode.ethdebug",
                "evm.deployedBytecode.object",
                "evm.deployedBytecode.opcodes",
                "evm.deployedBytecode.sourceMap",
                "evm.deployedBytecode.functionDebugData",
                "evm.deployedBytecode.generatedSources",
                "evm.deployedBytecode.linkReferences",
                "evm.deployedBytecode.immutableReferences",
                "evm.deployedBytecode.ethdebug",
                "ethdebug.resources",
                "ethdebug.compilation"
            ]"#,
        );

        assert_eq!(flags, OutputSelectionFlags::all());
    }

    #[test]
    fn output_selection_parent_keys() {
        assert_eq!(selection_flags(r#"["evm"]"#), OutputSelectionFlags::EVM);
        assert_eq!(
            selection_flags(r#"["evm.bytecode", "evm.deployedBytecode"]"#),
            OutputSelectionFlags::BYTECODE | OutputSelectionFlags::DEPLOYED_BYTECODE
        );
        assert!(OutputSelectionFlags::BYTECODE.contains(OutputSelectionFlags::BYTECODE_SOURCE_MAP));
        assert!(
            OutputSelectionFlags::DEPLOYED_BYTECODE
                .contains(OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP)
        );
        assert!(OutputSelectionFlags::WILDCARD.contains(
            OutputSelectionFlags::BYTECODE_SOURCE_MAP
                | OutputSelectionFlags::DEPLOYED_BYTECODE_SOURCE_MAP
        ));
        assert!(!OutputSelectionFlags::EVM.intersects(OutputSelectionFlags::ETHDEBUG));
    }

    #[test]
    fn output_selection_wildcard_and_unknown_keys() {
        assert_eq!(selection_flags(r#"["unknown", "*"]"#), OutputSelectionFlags::WILDCARD);
        assert_eq!(
            selection_flags(r#"["*", "ir", "evm.bytecode.ethdebug"]"#),
            OutputSelectionFlags::WILDCARD
                | OutputSelectionFlags::IR
                | OutputSelectionFlags::BYTECODE_ETHDEBUG
        );
        assert!(selection_flags(r#"["unknown", "evm.bytecode.unknown"]"#).is_empty());
    }

    #[test]
    fn output_selection_merges_source_and_contract_wildcards() {
        let selection = serde_json::from_str::<OutputSelection<'_>>(
            r#"{
                "A.sol": {
                    "A": ["abi"],
                    "*": ["userdoc"]
                },
                "*": {
                    "A": ["devdoc"],
                    "*": ["storageLayout"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            selection.contract("A.sol", "A"),
            OutputSelectionFlags::ABI
                | OutputSelectionFlags::USERDOC
                | OutputSelectionFlags::DEVDOC
                | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(
            selection.contract("A.sol", "B"),
            OutputSelectionFlags::USERDOC | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(
            selection.contract("B.sol", "A"),
            OutputSelectionFlags::DEVDOC | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(selection.contract("B.sol", "B"), OutputSelectionFlags::STORAGE_LAYOUT);
    }

    #[test]
    fn output_selection_contract_scope() {
        let selection = serde_json::from_str::<OutputSelection<'_>>(
            r#"{
                "A.sol": {
                    "A": ["ast", "abi", "ethdebug.compilation"]
                },
                "*": {
                    "A": ["devdoc", "ethdebug.compilation"],
                    "*": ["storageLayout", "ethdebug.resources"]
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            selection.contract("A.sol", "A"),
            OutputSelectionFlags::ABI
                | OutputSelectionFlags::DEVDOC
                | OutputSelectionFlags::STORAGE_LAYOUT
        );
        assert_eq!(selection.global(), OutputSelectionFlags::ETHDEBUG_RESOURCES);
    }
}
