# EVM rewrite API and contract appendix

This is a specification for deleting and rebuilding every tracked file under
`crates/codegen/src/backend/evm/`. It preserves interfaces and observable behavior,
not implementation bodies. Paths below are repository-relative. Retain callers,
MIR, analyses, transforms, memory policy, tests and benchmark artifacts.

## Required public surface

Type names below mean `solar_sema::Gcx`, `solar_config::EvmVersion`,
`solar_interface::{Session, Symbol, Result}`, `solar_interface::source_map::SourceFile`,
`alloy_primitives::U256`, and `std::fmt::Display`. `mir` means `solar_codegen::mir`.
Preserve visibility, lifetimes, argument order, return types and trait bounds.

| Import path / item | Declaration contract |
| --- | --- |
| `solar_codegen::Backend` and `solar_codegen::backend::Backend` | Associated `type Output`; `fn lower_module(&mut self, module: &mut mir::Module) -> Self::Output` |
| `solar_codegen::EvmCodegen` and `solar_codegen::backend::evm::EvmCodegen` | `pub struct EvmCodegen<'gcx>`; implements `Backend<Output = EvmArtifact>` |
| `EvmCodegen` methods | `pub fn new(gcx: Gcx<'gcx>) -> Self`; `pub fn set_capture_evm_ir(&mut self, capture: bool)`; `pub fn generate_deployment_bytecode(&mut self, module: &mut mir::Module) -> (Vec<u8>, Vec<u8>)` |
| `solar_codegen::backend::evm::EvmArtifact` | `Clone + Debug + Default`; public fields `deployment: Vec<u8>`, `runtime: Vec<u8>`, `deployment_evm_ir: Option<ir::Module>`, `runtime_evm_ir: Option<ir::Module>` |
| `solar_codegen::backend::evm` functions | `pub fn disassemble(bytecode: &[u8], evm_version: EvmVersion) -> String`; same signature for `disassemble_standard_json` |
| `solar_codegen::backend::evm::ir::Module` | `Clone + Debug + Default + PartialEq + Eq`; fields are not public API |
| `ir::Module` methods | `pub fn parse(sess: &Session, source: &SourceFile) -> Result<Self>`; `pub fn into_bytecode(self, gcx: Gcx<'_>) -> Result<Vec<u8>>`; `pub const fn name(&self) -> Symbol`; `pub fn to_text(&self) -> impl Display + '_` |
| `ir::validate` | `pub fn validate(gcx: Gcx<'_>, module: &Module)` |
| `ir::EvmPass` | `pub trait EvmPass: Sync`; methods `fn name(&self) -> &'static str`, `fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool`, `fn is_required(&self) -> bool`, `fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool` |
| `ir::ALL_PASSES` | `pub static ALL_PASSES: &[&dyn EvmPass]` |
| `ir::lookup_pass` | `pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass>` |
| `ir::run_passes` | `pub fn run_passes(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass], name: Option<&str>) -> bool` |
| `ir::run_passes_no_validate` | `pub fn run_passes_no_validate(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass]) -> bool` |
| `ir::run_pipeline` | `pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool` |
| `ir::pipeline_label` | Re-export retained `pass_manager::pipeline_label`; signature `pub fn pipeline_label(value: &str) -> &str` |

Keep default trait methods for `is_enabled` and `is_required`: required passes
are enabled at every optimization level; other passes are disabled at `-O0`.
`run_pass` and pipeline booleans report whether IR changed, not success.

The public pass-name inventory, in registry order, is `block-cse`, `peephole`,
`dce`, `reorder-pushes`, `share-reverts`, `stack-dedup`, `stack-normalize`,
`compact-pushes`, `constant-data`, `coalesce-copies`, `pack-data`, `legalize-shifts`,
`cfg-simplify`, `outline`, `terminal-dedup`, `tail-merge`, `block-layout`.
Preserve command-line names and their tested behavior; pipeline ordering is a
measured design choice. `pack-existing-data` is an internal pipeline pass, not a
public registry entry. Retain pass-manager handling of `none` and pipeline controls.

## Compiling unsupported stub and retained dependencies

Codegen entry points must emit an ordinary error through `Gcx::dcx()` and return
an empty default artifact (or empty tuple), with no MIR mutation. Use an explicit
message such as `EVM code generation is unavailable during backend reconstruction`.
Never return empty bytecode as apparent successful compilation. `contract.rs`
already checks diagnostics after `lower_module`; direct callers must also do so.
Setters can store flags. The crate-visible `set_capture_mir(&mut self, capture: bool)`
must remain callable by `contract.rs`. Do not introduce a legacy execution path.

Parsing and assembly stubs emit errors and return the emitted `ErrorGuaranteed`.
Validation and pass execution emit errors; pass execution returns `false`.
Known pass names can resolve to fresh unsupported descriptors, whose `run_pass`
also emits an error. Ensure pipeline execution diagnoses even when a pass would
be disabled. Text/disassembly APIs cannot return errors: temporarily return a
clearly marked unsupported string; callers with diagnostic context must reject
requests for unavailable output. Do not panic, fabricate valid output or silently
treat an unimplemented pass as successful. Restore these temporary behaviors by milestone.

Retain `EvmArtifact`'s crate-visible `immutable_references` collection and a fresh
record with `id: mir::ImmutableId`, `code_offset: usize`, `type_size: mir::TypeSize`.
`contract.rs` reads these fields; its public offset is `code_offset + 1`, because
the backend offset points at the placeholder PUSH opcode. This record need not
remain under the old private assembler path.

`utils/eval.rs` consumes opcode constants, `op::stack_io(opcode: u8) -> Option<(u8, u8)>`
and `mir::InstKind::evm_opcode(&self) -> Option<u8>`; both methods are crate-visible
and currently `const`. `contract.rs` also consumes `PUSH1`, `PUSH20`, `PUSH32`.
Recreate declarative opcode facts afresh from retained MIR and target specifications.
During the unsupported milestone, returning `None` from `evm_opcode` safely disables
that folding path; do not invent opcode mappings. Preserve the constants still
referenced in `utils/eval.rs`. Restore folding compatibility before acceptance.

`lower/data.rs` requires crate-visible `op::WORD_BYTES: usize = 32`,
`op::push_len(evm_version: EvmVersion, value: U256) -> usize`, and
`ir::immediate_materialization_cost(evm_version: EvmVersion, value: U256) -> (usize, usize)`.
The cost pair is encoded bytes and static gas. A fresh conservative literal-PUSH
cost is sufficient temporarily: zero uses PUSH0 only on supporting forks; other
values use the smallest legal PUSH width. Do not retain the old cost optimizer.
Other crate-private helpers and types used solely within the deleted directory
are not compatibility requirements; redesign or eliminate them freely.
