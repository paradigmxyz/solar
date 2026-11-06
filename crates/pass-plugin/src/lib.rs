//! Rust frontend and optimization plugins. Implement [`Plugin`] and use [`export_plugin!`] to build a
//! loadable Rust library. Plugins and the compiler must use the same SDK source, Rust toolchain,
//! target, profile, and Rust flags; the loader checks their fingerprints before calling Rust code.
//!
//! Requests are borrowed from the host. Responses are borrowed while the host clones them, so
//! each side retains ownership of its allocations. The export adapter contains plugin panics
//! when built with unwinding. Native libraries are trusted code, not a sandbox.

#[cfg(feature = "host")]
pub mod host;

/// Interface version understood by this SDK.
pub const API_VERSION: u32 = 1;

include!(concat!(env!("OUT_DIR"), "/abi.rs"));

/// Rust toolchain, SDK source, build configuration, and panic-strategy identity.
pub const ABI_FINGERPRINT: [u8; 32] = __private::build_fingerprint(cfg!(panic = "unwind"));

/// Implemented by an out-of-tree Rust plugin. Its methods execute inside the plugin; the export
/// macro lends their results to the compiler and drops them before returning to the host.
pub trait Plugin: Sync {
    fn manifest(&self) -> Manifest;
    /// Runs an IR pass; frontend-only libraries may keep the default no-op implementation.
    fn run(&self, _request: &Request) -> Result<Response, String> {
        Ok(Response::default())
    }

    /// Lowers a registered builtin to an acyclic pure word fragment.
    fn lower_builtin(&self, _name: &str) -> Result<BuiltinBody, String> {
        Err("unknown plugin builtin".into())
    }

    /// Describes frontend extensions made available by this plugin.
    fn frontend_manifest(&self) -> FrontendManifest {
        FrontendManifest::default()
    }

    /// Expands a frontend invocation into host-allocated syntax.
    fn expand(&self, _request: &ExpansionRequest) -> Result<Expansion, String> {
        Err("unknown frontend extension".into())
    }
}

/// Frontend extension registration, qualified by the plugin's manifest name.
#[derive(Clone, Debug, Default)]
pub struct FrontendManifest {
    pub extensions: Vec<FrontendExtension>,
    pub builtins: Vec<BuiltinDefinition>,
}

#[derive(Clone, Debug)]
pub struct FrontendExtension {
    pub name: String,
    pub kind: ExtensionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionKind {
    Expression,
    Statement,
}

/// A source token's byte range is relative to the source file, with an exclusive end.
#[derive(Clone, Debug)]
pub struct FrontendToken {
    pub text: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug)]
pub struct ExpansionRequest {
    pub name: String,
    pub kind: ExtensionKind,
    pub tokens: Vec<FrontendToken>,
}

/// Declarative syntax nodes. Inputs refer to parsed arguments or the parsed body; temporary
/// variables are allocated by the compiler, so their names cannot collide with user names.
#[derive(Clone, Debug)]
pub enum Expansion {
    Expression(ExpansionExpr),
    Statement(ExpansionStmt),
    Builtin(BuiltinSignature),
}

#[derive(Clone, Debug)]
pub enum ExpansionExpr {
    Argument(u32),
    Uint(String),
    Binary { op: String, left: Box<Self>, right: Box<Self> },
}

#[derive(Clone, Debug)]
pub enum ExpansionStmt {
    Body,
    Block(Vec<Self>),
    /// Evaluate `count` once, then execute `body` that many times.
    Repeat {
        count: ExpansionExpr,
        body: Box<Self>,
    },
}

/// Typed scalar builtins. This version accepts only pure word-to-word fragments.
#[derive(Clone, Debug)]
pub struct BuiltinDefinition {
    pub name: String,
    pub signature: BuiltinSignature,
}

#[derive(Clone, Debug)]
pub struct BuiltinSignature {
    pub parameters: Vec<BuiltinType>,
    pub result: BuiltinType,
    pub effect: BuiltinEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinType {
    Uint256,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuiltinEffect {
    #[default]
    Unknown,
    Pure,
}

/// Operands refer to arguments, prior instructions, or word constants.
#[derive(Clone, Debug)]
pub enum BuiltinOperand {
    Argument(u32),
    Local(u32),
    Constant(String),
}

#[derive(Clone, Debug)]
pub struct BuiltinInstruction {
    pub operation: String,
    pub operands: Vec<BuiltinOperand>,
}

#[derive(Clone, Debug)]
pub struct BuiltinBody {
    pub instructions: Vec<BuiltinInstruction>,
    pub result: BuiltinOperand,
}

/// IR layer accepted by a pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ir {
    Mir,
    EvmIr,
}

/// One named pass exported by a plugin.
#[derive(Clone, Debug)]
pub struct Pass {
    pub name: String,
    pub ir: Ir,
    /// MIR requires `semantic` or `lowered`. EVM IR requires no phase.
    pub phase: Option<String>,
    /// Run at the default extension point when the library is explicitly loaded.
    pub enabled_by_default: bool,
}

/// Registration response. Pass names are qualified by `name`, as `name::pass`.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub api_version: u32,
    pub name: String,
    pub version: String,
    pub passes: Vec<Pass>,
}

/// Request from the compiler to a plugin.
#[derive(Clone, Debug)]
pub struct Request {
    pub pass: String,
    pub evm_version: String,
    pub optimization: String,
    pub module: Snapshot,
}

/// Read-only view. Numeric identities are invocation-local, not addresses or persistent IDs.
#[derive(Clone, Debug)]
pub enum Snapshot {
    Mir {
        name: String,
        phase: String,
        /// Whether memory writes must be treated as potentially unstructured.
        unsafe_assembly: bool,
        text: String,
        functions: Vec<Function>,
    },
    EvmIr {
        name: String,
        text: String,
        blocks: Vec<EvmBlock>,
    },
}

/// MIR function with its active instructions and values.
#[derive(Clone, Debug)]
pub struct Function {
    pub id: u32,
    pub name: String,
    pub optimization: String,
    pub blocks: Vec<u32>,
    pub instructions: Vec<MirInstruction>,
    pub values: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct MirInstruction {
    pub id: u32,
    pub block: u32,
    pub operation: String,
    pub operands: Vec<u32>,
    pub result: Option<u32>,
    pub result_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Value {
    pub id: u32,
    pub ty: String,
    /// A scalar constant in hexadecimal, or `None` for a nonconstant value.
    pub constant: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EvmBlock {
    pub id: u32,
    pub label: u32,
    pub instructions: Vec<EvmInstruction>,
}

#[derive(Clone, Debug)]
pub struct EvmInstruction {
    pub index: u32,
    pub text: String,
    pub opcode: u8,
    pub immediate: Option<String>,
    pub protected: bool,
}

/// Edits applied sequentially to a private candidate, then verified before committing it.
#[derive(Clone, Debug, Default)]
pub struct Response {
    pub edits: Vec<Edit>,
}

/// Edits preserve the representation phase and existing module declarations. New internal
/// functions, blocks, SSA definitions, and control-flow transfers are verified as one transaction.
#[derive(Clone, Debug)]
pub enum Edit {
    ReplaceMirValue {
        function: u32,
        value: u32,
        replacement: u32,
    },
    RewriteMirInstruction {
        function: u32,
        instruction: u32,
        operation: String,
        operands: Vec<u32>,
    },
    SpliceEvmInstructions {
        block: u32,
        start: u32,
        remove: u32,
        instructions: Vec<NewEvmInstruction>,
    },
    AddMirFunctions {
        functions: Vec<NewMirFunction>,
    },
    AddMirBlocks {
        function: u32,
        blocks: Vec<NewMirBlock>,
    },
    SplitMirBlock {
        function: u32,
        block: u32,
        at: u32,
    },
    ReplaceMirValueWith {
        function: u32,
        value: u32,
        replacement: MirOperand,
    },
    SpliceMirInstructions {
        function: u32,
        block: u32,
        start: u32,
        remove: u32,
        instructions: Vec<NewMirInstruction>,
    },
    SetMirTerminator {
        function: u32,
        block: u32,
        terminator: MirTerminator,
    },
    AddEvmBlocks {
        blocks: Vec<NewEvmBlock>,
    },
    SplitEvmBlock {
        block: u32,
        at: u32,
    },
    SetEvmTerminator {
        block: u32,
        terminator: EvmTerminator,
    },
}

/// Existing instructions can be copied with their relocations. Newly constructed instructions
/// can inherit a known source instruction's debug context, or explicitly remain unmapped.
#[derive(Clone, Debug)]
pub enum NewEvmInstruction {
    Copy { index: u32 },
    CopyFrom { block: u32, index: u32 },
    BlockAddress { block: u32 },
    Opcode { opcode: u8, source: Option<u32> },
    Push { value: String, source: Option<u32> },
}

/// Types for new MIR signatures and SSA results. Struct IDs refer to the existing type table.
#[derive(Clone, Debug)]
pub enum Type {
    Integer { bits: u32 },
    Pointer { location: AddressSpace },
    Slice { location: AddressSpace },
    Struct { id: u32 },
    Void,
}

#[derive(Clone, Copy, Debug)]
pub enum AddressSpace {
    Memory,
    Calldata,
    Returndata,
    Storage,
    Transient,
}

/// Local IDs name results created by this response, scoped to one function. They are distinct
/// from the compiler's existing value IDs. Constants must fit their declared scalar type.
#[derive(Clone, Debug)]
pub enum MirOperand {
    Value { id: u32 },
    Argument { index: u32 },
    Local { id: u32 },
    Constant { ty: Type, value: String },
}

/// New internal functions append in order, so their IDs start at the snapshot's function count.
#[derive(Clone, Debug)]
pub struct NewMirFunction {
    pub name: String,
    pub parameters: Vec<Type>,
    pub returns: Type,
    pub blocks: Vec<NewMirBlock>,
}

/// Blocks append in order; block IDs start at zero for a new function. All blocks and result
/// locals in a batch are declared before their bodies, allowing forward edges and loop phis.
#[derive(Clone, Debug)]
pub struct NewMirBlock {
    pub instructions: Vec<NewMirInstruction>,
    pub terminator: MirTerminator,
}

#[derive(Clone, Debug)]
pub struct NewMirInstruction {
    pub result: Option<u32>,
    pub ty: Option<Type>,
    pub instruction: MirInstructionKind,
}

#[derive(Clone, Debug)]
pub enum MirInstructionKind {
    Operation {
        operation: String,
        operands: Vec<MirOperand>,
    },
    Phi {
        incoming: Vec<(u32, MirOperand)>,
    },
    Call {
        function: u32,
        args: Vec<MirOperand>,
    },
    /// Clone an existing operation's attributes and debug context, replacing its operands.
    Copy {
        instruction: u32,
        operands: Vec<MirOperand>,
    },
}

#[derive(Clone, Debug)]
pub enum MirTerminator {
    Jump { target: u32 },
    Branch { condition: MirOperand, then_block: u32, else_block: u32 },
    Switch { value: MirOperand, default: u32, cases: Vec<(MirOperand, u32)> },
    Return { values: Vec<MirOperand> },
    ReturnData { offset: MirOperand, size: MirOperand },
    Revert { offset: MirOperand, size: MirOperand },
    TailCall { function: u32, args: Vec<MirOperand> },
    Stop,
    Invalid,
}

/// EVM IR has no function objects. Callable helpers are blocks with explicit stack arguments,
/// return-address pushes, dynamic return jumps, and optional verified call-effect annotations.
#[derive(Clone, Debug)]
pub struct NewEvmBlock {
    pub instructions: Vec<NewEvmInstruction>,
    pub terminator: EvmTerminator,
    pub call_effect: Option<CallEffect>,
}

#[derive(Clone, Debug)]
pub struct CallEffect {
    pub ret: u8,
    pub inputs: u8,
    pub outputs: u8,
}

#[derive(Clone, Debug)]
pub enum EvmTerminator {
    Jump { target: u32 },
    Branch { then_block: u32, else_block: u32 },
    IndexedJump { targets: Vec<u32> },
    Opcode { opcode: u8 },
}

/// Exports a Rust plugin and its compatibility fingerprint. A unit struct needs only its name:
/// `solar_pass_plugin::export_plugin!(MyPlugin)`. Stateful plugins pass a type and a constant
/// initializer: `solar_pass_plugin::export_plugin!(MyPlugin, MyPlugin::new())`.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ident) => {
        $crate::export_plugin!($plugin, $plugin);
    };
    ($plugin:ty, $instance:expr) => {
        #[unsafe(no_mangle)]
        pub static solar_rust_pass_plugin_abi_v1: [u8; 32] =
            $crate::__private::build_fingerprint(cfg!(panic = "unwind"));

        #[unsafe(no_mangle)]
        pub fn solar_rust_pass_plugin_v1() -> &'static $crate::__private::Exports {
            static PLUGIN: $plugin = $instance;
            static EXPORTS: $crate::__private::Exports = $crate::__private::Exports {
                manifest: |respond| $crate::__private::manifest(&PLUGIN, respond),
                run: |request, respond| $crate::__private::run(&PLUGIN, request, respond),
                lower_builtin: |name, respond| {
                    $crate::__private::lower_builtin(&PLUGIN, name, respond)
                },
                frontend_manifest: |respond| $crate::__private::frontend_manifest(&PLUGIN, respond),
                expand: |request, respond| $crate::__private::expand(&PLUGIN, request, respond),
            };
            &EXPORTS
        }
    };
}

/// Implementation details shared by the loader and export macro, not a second plugin API.
#[doc(hidden)]
pub mod __private {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    pub const ENTRY_SYMBOL: &[u8] = b"solar_rust_pass_plugin_v1\0";
    pub const ABI_SYMBOL: &[u8] = b"solar_rust_pass_plugin_abi_v1\0";
    pub type Entry = fn() -> &'static Exports;
    pub type Receiver<'a, T> = dyn FnMut(Result<&T, &str>) + 'a;

    pub struct Exports {
        pub manifest: fn(&mut Receiver<'_, Manifest>),
        pub run: fn(&Request, &mut Receiver<'_, Response>),
        pub lower_builtin: fn(&str, &mut Receiver<'_, BuiltinBody>),
        pub frontend_manifest: fn(&mut Receiver<'_, FrontendManifest>),
        pub expand: fn(&ExpansionRequest, &mut Receiver<'_, Expansion>),
    }

    pub const fn build_fingerprint(panic_unwind: bool) -> [u8; 32] {
        let mut fingerprint = SDK_BUILD_FINGERPRINT;
        // Include the exporting crate's actual panic strategy as well as the SDK build's.
        fingerprint[0] ^= panic_unwind as u8;
        fingerprint
    }

    pub fn manifest<P: Plugin>(plugin: &P, respond: &mut Receiver<'_, Manifest>) {
        match catch_unwind(AssertUnwindSafe(|| plugin.manifest())) {
            Ok(manifest) => respond(Ok(&manifest)),
            Err(_) => respond(Err("plugin panicked")),
        }
    }

    pub fn lower_builtin<P: Plugin>(
        plugin: &P,
        name: &str,
        respond: &mut Receiver<'_, BuiltinBody>,
    ) {
        match catch_unwind(AssertUnwindSafe(|| plugin.lower_builtin(name))) {
            Ok(Ok(response)) => respond(Ok(&response)),
            Ok(Err(error)) => respond(Err(&error)),
            Err(_) => respond(Err("plugin panicked")),
        }
    }

    pub fn frontend_manifest<P: Plugin>(plugin: &P, respond: &mut Receiver<'_, FrontendManifest>) {
        match catch_unwind(AssertUnwindSafe(|| plugin.frontend_manifest())) {
            Ok(manifest) => respond(Ok(&manifest)),
            Err(_) => respond(Err("plugin panicked")),
        }
    }

    pub fn expand<P: Plugin>(
        plugin: &P,
        request: &ExpansionRequest,
        respond: &mut Receiver<'_, Expansion>,
    ) {
        match catch_unwind(AssertUnwindSafe(|| plugin.expand(request))) {
            Ok(Ok(response)) => respond(Ok(&response)),
            Ok(Err(error)) => respond(Err(&error)),
            Err(_) => respond(Err("plugin panicked")),
        }
    }

    pub fn run<P: Plugin>(plugin: &P, request: &Request, respond: &mut Receiver<'_, Response>) {
        match catch_unwind(AssertUnwindSafe(|| plugin.run(request))) {
            Ok(Ok(response)) => respond(Ok(&response)),
            Ok(Err(error)) => respond(Err(&error)),
            Err(_) => respond(Err("plugin panicked")),
        }
    }
}
