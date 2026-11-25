# Operation coverage

The compiler schemas define the scope: 107 MIR operation families and 154 EVM
opcode entries, across 15 forks. `solar-tv-replay --catalog` reflects those
schemas, including stack effects, availability and gas. Tests compare every entry
with [operations.json](operations.json), so new operations cannot go unnoticed.

| Status | MIR families | EVM opcode entries |
| --- | ---: | ---: |
| Checked within the supported fragment | 32 | 103 |
| Some forms checked | 9 | 0 |
| Explicit-world entry execution | 0 | 51 |
| Direct typed MIR state execution | 66 | 0 |
| Missing | 0 | 0 |

The counts are **not a claim of complete operation support**. PUSH/DUP/SWAP
variants account for many EVM entries. Checking a word operation does not cover
arbitrary physical PCs, memory addresses or every MIR layout using that operation.

An operation is complete only when its meaning is defined in Lean, the importer
preserves that meaning, independent replay covers it, and boundary tests pass.
Naming it, returning an arbitrary value, trusting a successful external call, or
treating an effect as a no-op does not implement it.

Source-graph identity certificates do not increase these operation counts. They
retain identical operations under shared node semantics, without inventing
values or discarding effects. They do not enable rewrites of an unsupported
operation or establish physical transfer prices.

Cyclic MIR DCE has a separate contextual action-machine simulation. It can
erase unused total scalar or slice definitions. Every retained operation,
simultaneous phi group and transfer stays exact, with
arbitrary shared state and failing handlers. The theorem covers every logical
block-transfer bound. Physical gas, termination, the parser and the action
renderer are outside the claim. Six real DCE captures verify under this
certificate. The latest full audit verifies 1,392 of 2,228 captures at
three seconds per prover invocation. It records 569 unsupported checks,
267 timeouts and 0 failed checks. Per-pass counts and fingerprints are
recorded in [CORRECTNESS.md](CORRECTNESS.md#full-corpus-after-mir-and-backend-coverage).

## Implemented fragment

All 26 pure EVM word operations are modeled, including zero division, signed
minimum divided by -1, widened modular arithmetic, shifts at and above 256,
SIGNEXTEND, BYTE and CLZ. Exponentiation uses squaring, with a kernel proof that
it equals ordinary bit-vector power for every exponent. MIR uses native bit
vectors from i1 through i256, signed comparisons, casts, select and checked
arithmetic, including division, remainder and powers with exact panic codes.

Local memory windows support word/byte reads and writes, overlapping MCOPY,
Keccak-256, expansion and terminal byte payloads within a 256-byte projection.
Explicit-world entries use ordinary byte memory with a 64 KiB model limit;
they are not restricted to that local projection. MIR copies are logical
memmoves, including before Cancun; EVM MCOPY remains fork-gated.
The projected read, active-size and expansion-charge lemmas bind MLOAD,
KECCAK256, RETURN and REVERT to fetched frame instructions for a chosen prefix
length, including the full 64 KiB domain. Their footprint, configuration and
frame alignment premises stay explicit. The corresponding MSTORE, MSTORE8 and
MCOPY frame equations prove preservation beyond the write footprint and the
exact expansion charge, including overlapping and zero-length copies.
Whole-CFG correspondence remains open.
Keccak has an executable permutation and original Keccak padding; it has no
uninterpreted hash oracle or collision axiom. Kernel-checked vectors include
empty input, `abc`, and both sides of the 136-byte absorption boundary.

Gas prices for the admitted window operations are fork-dependent. Tests check
pure-op prices and EXP byte charges against the compiler's table on every fork.
Availability is checked before proving a pair, including semantic MIR opcodes.
This does not yet provide full transaction gas semantics.

## State and calls

[State/Storage.lean](SolarTV/State/Storage.lean) defines address-qualified
persistent/original/transient stores, access warmth, signed refunds, static
restrictions and the SSTORE sentry. Tests distinguish Constantinople's net
metering, Petersburg's legacy pricing, Berlin cold accesses and London refunds.
Storage state uses total functions, not a silently bounded array of slots.

[State/World.lean](SolarTV/State/World.lean) contains accounts and journaled
transaction effects. [State/Calls.lean](SolarTV/State/Calls.lean) separates
message entry from completion: CALL changes storage owner and transfers value;
CALLCODE changes code; DELEGATECALL inherits caller/value/owner; STATICCALL
forces static mode. Reverted children restore the journal and retain return data;
exceptional children restore the journal and burn their gas.

[EVMIR/State/](SolarTV/EVMIR/State/) connects those definitions to complete-entry
execution with `--world`. The Lean evaluator executes nested call frames and
CREATE/CREATE2 constructors, real address derivation, nonce/collision rules,
code-deposit costs, deployment restrictions, storage and journal rollback.
Independent `evm2` replay rebuilds both original IR inputs through the compiler.

All 18 precompiles active during Istanbul–Osaka execute in pure Lean. This
includes SHA-256, RIPEMD-160, identity, BLAKE2F, ModExp, ECRECOVER, P256VERIFY,
BN254 operations, BLS12-381 ADD/MSM/pairing/maps and KZG point evaluation. KZG
uses Ethereum's public ceremony setup; field maps use the EIP-2537 isogeny
constants. No crypto result is imported through FFI or an uninterpreted oracle.
Fork activation, gas, canonical encodings, subgroup rules and malformed inputs
are checked against the native engine. Expensive model computations can still
exceed the prover's work/time limits and remain inconclusive.

SELFDESTRUCT models the London refund removal and Cancun EIP-6780 distinction
between existing and newly created accounts. Deletion is deferred, balance
changes are immediate, and nested reverts restore the complete journal.

Environment reads use explicit block/transaction fixtures: fork-selected
difficulty/randomness, block history, fees, blob hashes, chain ID and SLOTNUM.
EXTCODECOPY reads account bytes with padding and memory/access gas. From Prague,
EIP-7702 designators resolve once for execution; their target access costs/warmth
and call contexts are replayed independently. Authorization processing precedes
the explicit-world entry model. A separate transaction checker handles decoded
set-code envelopes, signed authorizations and fee settlement through Osaka.
Amsterdam admits environment/scalar/memory execution and reservoir certificates for
debits, refills, rollback and child ledger merging. An explicit bytecode entry
also checks `SLOAD` and `SSTORE` with independent execution and reservoir
balances. Account access, message and creation opcodes still need Amsterdam
metering.

[MIR/State/](SolarTV/MIR/State/) executes typed SSA directly, with scalar memory
pointers, persistent/transient cells, context, logs, terminal outcomes and cyclic
CFGs. Phi inputs are read simultaneously. It shares byte memory, accounts and
journals with EVM execution through [State/](SolarTV/State/), without importing
physical stack layout or EVM gas. Real `storage-cse`/`storage-dse` pairs and broken
writes are checked against the Lean kernel and independent original-MIR replay.
Direct typed internal calls, tail calls, FMP reads, checked/infallible allocations
and byte-exact initialization execute without backend placements. Callers share
the journal and byte memory; their SSA environments are separate and width-typed.
Real inlining and allocation-lowering pairs exercise generated helpers, with
broken-call and object-aliasing counterexamples. External call families, creation
and return-data operations execute real fixture children through the shared message boundary and independent native message API.
Explicit caller boundary budgets price forwarding and creation; without them,
CALL requires an explicit request and sufficient caller gas to avoid clipping,
while CREATE uses explicit child budgets. Native replay never lowers the MIR caller. Live
returndata, access warmth and account lifetime flags are preserved at internal
returns. Child gas and nested work limits remain real, not assumed successful.
Products retain their nested fields and undefined components; insertion,
projection, aggregate phis/selects and logical internal-call results execute
directly. Memory/calldata/returndata slices are location-tagged pointer/length
pairs. Real `lower-structs` and `lower-slices` pairs compare logical boundary
leaves, without using their lowering as the semantic definition. Types are
limited to depth 32 and 256 nodes, with at most 65,536 exported type nodes
per model; boundaries have at most 32 input and 256
return leaves. Reading an undefined scalar remains unsupported.
Slot derivation executes Keccak with its scratch stores, transient FMP-based
byte-key copies, memory expansion and wrapping arithmetic. Storage byte helpers
validate short/long headers before allocation or mutation, retain full-word reads,
mask dirty padding and clear truncated long tails. Clearing and copying beyond
the documented memory or 2048-word storage limits are unsupported, never truncated.
Kernel proofs check the emitted load, store and clear loops and compose their
logical effects with the retained caller table of the pinned ERC20Mock capture.
The load proof covers slot warmth, the allocation cursor, live byte memory and
exact panic payloads. The typed expansion matcher and renderer remain trusted.
Storage-array loads reserve a word-strided memory array, unpack signed integers
and left-aligned fixed bytes, check enum bounds, and allocate nested bytes objects
in order. Packing ignores dirty unused bits and reads only the declared elements;
work beyond 2048 elements is unsupported. Kernel proofs cover real builtin expansion on small arrays; native differential
tests also cover full packed words and mixed nested byte-string arrays. These
larger replay cases are not counted as Lean proofs.
External scratch frame slots use their convention-defined base of 128 and alias
ordinary memory, including across internal calls. Slices store pointer then
length; the multi-return word is the buffer-pointer slot at 32. Internal frames
use explicit per-function live-activation bases, including recursion and tail
replacement. Physical multi-return buffers have explicit per-function
placements and retain the caller's return owner through tail calls. Logical
outer boundaries compare all return components. A supplied placement also
selects physical publication for nested products and slices, in declared leaf
order. Callers keep their typed SSA value before lowering; expanded callers
recover extras through the same bytes. Overlapping buffers retain snapshots and
store order, and an unused first word may remain undefined. Non-word or
nonzero-offset multi-return frame slots are invalid MIR, rejected by the verifier
before lowering. Frame-load result types
must match the slot, and convention-relative offsets must fit in `u64`.
Deferred allocations use explicit static-address or FMP-backed site bindings;
extents must match the IR and initialization touches the actual bytes. No
captured backend memory records are imported as semantic evidence.
Typed address calls support memory objects and slices for CALL,
STATICCALL and DELEGATECALL. Object headers are read before the message; slices
need no header or allocation. Returndata objects copy exactly the live buffer,
without clearing reserved padding or consuming the buffer. Implicit gas requires
an explicit caller budget and EIP-150; it forwards the cap after boundary charges.
External balance/code reads preserve address truncation and access warmth;
code introspection sees original delegation designators, not their targets.
Constant `datacopy` executes known bytes and patches library addresses. Deferred
entries require an explicit code image and linked extent; the verifier's exact
`datasize` rule remains in force. Known entries must agree with supplied image
bytes after patching. `codecopy` handles neighboring bytes and zero padding.
The total exported-data budget is 64 KiB; missing link inputs are not guessed.
Explicit MIR window contexts supply code images, linked extents, function heap/frame
placements, constructor ranges, library addresses and scalar immutable relocations.
Constructor immutable slots alias ordinary memory and use the declared SSA
carrier. Runtime relocations are read-only; signed values may be sign-extended,
typed reads keep their low carrier bits, and raw reads retain the full word.
Fixed-byte padding is retained. Invalid context representations are unsupported.
These are declared boundary inputs, not a proof that the backend chose them. A
missing binding, conflicting relocation or changed declaration table is unsupported.
GAS admits only a single-opcode sampling window with an explicit pre-op budget and
its own fork-specific charge; general MIR caller gas remains unsupported.
Recursive ABI tuples, dynamic/fixed arrays, bytes and function words preserve
allocation, header and selector store order in object, slice and scratch modes.
Decoders preserve signed length checks, canonicality, enum rejection and
function-word normalization. Reference inputs retain their declared memory,
calldata or storage semantics. Nested layouts have depth/node/work limits;
malformed input reverts, while exceeding a model budget is unsupported.
Validation markers carry an explicit canonicality rule and operand. Unsigned
widths, sign extension, fixed-byte padding, booleans and enum ranges are checked
on the reached path, including through phis and calls. False obligations are
unsupported, never assumed true or turned into a new guard. Raw external-function
padding (`bytes24`) and normalized function bits (`uint192`) are distinct facts.
Packed encodings admit literals, scalar words, byte references and word arrays,
including fixed/nested layouts with explicit strides. Object headers and slice
lengths are resolved before allocation, payload copies preserve aliasing and
cleanup order, and hash borrowing retains its header restoration. Trailing
stores and partial-word footprints remain observable. Real conversion-pass pairs are
kernel-checked and independently replayed from the original MIR on both sides.
Lowered function-pointer dispatchers are ordinary MIR functions; a real
dispatcher inlining pair and wrong-callee counterexample exercise that path.
Root runtime reentry and general backend layout correspondence remain unsupported.

## Partially admitted MIR families

These nine inventory entries still have restrictions beyond the common
work/memory limits. Their existing executable forms are tested; the restrictions
are not erased by the zero `Missing` count.

| Families | Admitted boundary | Remaining restriction | Regression coverage |
| --- | --- | --- | --- |
| `extcall`, `call`, `callcode`, `staticcall`, `delegatecall` | Real fixture children; optional typed inputs, scratch-word output and caller budgets that price boundary charges and EIP-150 forwarding | Abstract MIR work remains unpriced; omitted caller budgets require explicit requested gas and sufficient gas to avoid clipping | [Address calls](tests/test_mir_address_calls.py), [implicit gas](tests/test_mir_implicit_gas.py), [message gas](tests/test_mir_message_gas.py) |
| `create`, `create2` | Initcode execution and journal rollback; child gas derived from explicit caller budgets or supplied directly | Budgets are inputs, not a derivation from caller work | [Messages and creation](tests/test_mir_messages.py), [message gas](tests/test_mir_message_gas.py) |
| `icall` | Direct/dispatcher calls, typed products, aggregate/slice return buffers and builtin recipes | General call-site pricing and backend placement/stack-return correspondence | [Calls](tests/test_mir_calls.py), [aggregate returns](tests/test_mir_aggregate_returns.py), [return buffers](tests/test_mir_return_buffers.py), [builtins](tests/test_mir_builtins.py) |
| `gas` | One-opcode window with an explicit pre-op budget | General MIR caller gas | [Layout windows](tests/test_mir_layout.py) |

The world fixture is an explicit assumption. Declared symbolic slots, typed MIR
arguments and EVM entry stack words are universally quantified. Only EVM entries
use the trusted backend-image boundary. Audits retain this distinction and the
fixture/configuration hashes; direct MIR audits have no runtime-image hashes.
See the [world mode](README.md#storage-calls-and-creation)
for admitted forks, limits, observables and gas policies.

## Remaining work

Acyclic multi-block EVM regions now follow changed successors and shared tails
through terminal outcomes, with symbolic live-in stacks and memory. Internal
labels and stack layouts need not correspond. Pure cyclic regions with equal
independently exported source graphs also admit an all-execution-bounds proof.
General loop invariants, resolved dynamic targets, priced region transfers and
stateful instruction rewrites remain unsupported. Reordered graphs also admit
a kernel-checked mapping with opcode-schema height bounds: changed transfer
growth checks must be proved safe for every incoming stack. Payloads and
execution bounds remain arbitrary; physical transfer pricing and the concrete
opcode-handler embedding remain outside this certificate. The continuation
certificate can resolve a dynamic jump when its top stack word is a tracked
block label. It follows the actual label through ordinary stack shuffles and
checks that no value-consuming operation observes it. Unresolved jumps still
need an explicit output boundary. Physical return PCs, escaping labels,
external messages and code-image observers remain outside this proof.
The separate emitted indexed-table entry proof follows literal return PCs
through stack permutations and untouched suffixes. It checks each motion
against the bytecode interpreter, including a resumed parent's stack, and
retains the top 32 facts. A shared Solidity helper before a dense switch and
a changed return address have kernel coverage checks and independent raw-byte
replay. Computed or spilled addresses and backend placement remain open.
Identical independently exported stateful graphs have a separate structural
certificate for every execution bound; opcode and physical-layout bindings
remain trusted. See
the [region examples](README.md#multi-block-evm-ir-regions).

Complete unused MIR type-table erasure also has a module-level structural
certificate. It checks independently resolved typed data for every retained
function and preserves globals, phase and source text. This does not relax
normal GAS execution or prove primitive instruction implementations; see
the [type-pruning example](README.md#removing-unused-mir-type-declarations).

1. Generalize fixed-world proofs to arbitrary symbolic accounts and aliases.
   Derive general MIR message budgets from priced execution rather than supplied
   boundary inputs; model root reentry through MIR entry semantics.
2. Extend decoded set-code transaction admission to the other envelope types
   and prove full fee settlement and block accumulation.
3. Generalize calldata/returndata/program-data access and physical code layout
   from complete-entry execution to arbitrary local windows.
4. Extend the per-import correspondence certificates beyond acyclic EVM regions
   and acyclic typed scalar MIR. Connect explicit placement assumptions to
   actual backend layout decisions. Fresh-heap direct SSA proofs now preserve
   complete bodies under a named placement/FMP producer condition, with proved
   explicit initialization and non-vacuity. Establishing that condition at
   every caller remains open. Direct MIR semantics and independent replay do
   not by themselves prove backend decisions correct.
5. Integrate Amsterdam's reservoir initialization, debits, refills and rollback with
   stateful opcode execution, nested frames and transaction settlement. Expand
   differential boundary coverage.

Unsupported semantics remain explicit. A local proof does not establish
whole-contract equivalence or an all-input correspondence theorem for the Rust
implementation.

See [CORRECTNESS.md](CORRECTNESS.md) for the checked import boundaries, pinned
real-contract evaluation and the evidence required to close each remaining gap.
