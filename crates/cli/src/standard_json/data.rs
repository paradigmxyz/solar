//! Standard JSON data structures, serialization, selection parsing, and statistics.

use alloy_primitives::{Address, Bytes};
use indexmap::IndexMap;
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};
use serde_json::{Map, Value};
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
    // Metadata output is not supported yet.
    // #[serde(borrow, default)]
    // metadata: Option<CowValue<'a>>,
    #[serde(borrow, default)]
    pub(super) libraries: Libraries<'a>,
    // Debug output is not supported yet.
    // #[serde(borrow, default)]
    // debug: Option<CowValue<'a>>,
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

/// The supported subset of solc's Standard JSON `settings.optimizer` object.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Optimizer {
    /// Whether the optimizer is enabled.
    #[serde(default)]
    pub(super) enabled: bool,
    /// Number of optimizer runs. The MIR optimizer has no runs parameter yet.
    #[serde(default)]
    pub(super) runs: Option<u64>,
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
pub(super) struct CompilerOutput<'a> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) errors: Vec<SolcDiagnostic<'a>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub(super) sources: FxIndexMap<String, SourceOutput>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub(super) contracts: FxIndexMap<String, FxIndexMap<String, ContractOutput<'a>>>,
    // `ethdebug` output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ethdebug: Option<CowValue<'a>>,
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
pub(super) struct ContractOutput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) abi: Option<Vec<alloy_json_abi::AbiItem<'a>>>,
    // Metadata output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) userdoc: Option<Documentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) devdoc: Option<Documentation>,
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_hex_bytes"
    )]
    pub(super) object: Option<Bytes>,
    // Ethdebug output is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // ethdebug: Option<CowValue<'a>>,
    // Function debug data is not supported yet.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // function_debug_data: Option<CowValue<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) opcodes: Option<String>,
    // Source map output is not supported yet.
    // #[serde(default, skip_serializing_if = "String::is_empty")]
    // source_map: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) link_references: Option<FxIndexMap<String, FxIndexMap<String, Vec<OffsetLength>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) immutable_references: Option<FxIndexMap<String, Vec<OffsetLength>>>,
    //
    // Not supported.
    // #[serde(skip_serializing_if = "Option::is_none")]
    // generated_sources: Option<CowValue<'a>>,
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

fn serialize_optional_hex_bytes<S>(bytes: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(bytes) = bytes {
        serializer.serialize_str(&alloy_primitives::hex::encode(bytes))
    } else {
        serializer.serialize_none()
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
    }
}
