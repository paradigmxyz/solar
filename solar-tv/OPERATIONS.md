# Operation coverage

The compiler schemas define the scope: 107 MIR operation families and 154 EVM
opcode entries, across 15 forks. `solar-tv-replay --catalog` reflects those
schemas, including stack effects, availability and gas. Tests compare every entry
with [operations.json](operations.json), so new operations cannot go unnoticed.

| Status | MIR families | EVM opcode entries |
| --- | ---: | ---: |
| Checked within the supported fragment | 32 | 103 |
| Some forms checked | 18 | 6 |
| Explicit-world entry execution | 0 | 45 |
| Direct typed MIR state execution | 42 | 0 |
| Missing | 15 | 0 |

The counts are **not a claim of complete operation support**. PUSH/DUP/SWAP
variants account for many EVM entries. Checking a word operation does not cover
arbitrary physical PCs, memory addresses or every MIR layout using that operation.

An operation is complete only when its meaning is defined in Lean, the importer
preserves that meaning, independent replay covers it, and boundary tests pass.
Naming it, returning an arbitrary value, trusting a successful external call, or
treating an effect as a no-op does not implement it.

## Implemented fragment

All 26 pure EVM word operations are modeled, including zero division, signed
minimum divided by -1, widened modular arithmetic, shifts at and above 256,
SIGNEXTEND, BYTE and CLZ. Exponentiation uses squaring, with a kernel proof that
it equals ordinary bit-vector power for every exponent. MIR uses native bit
vectors from i1 through i256, signed comparisons, casts, select and checked
arithmetic, including division, remainder and powers with exact panic codes.

Memory supports word/byte reads and writes, overlapping MCOPY, Keccak-256,
expansion and terminal byte payloads within the existing 256-byte projection.
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
the entry model. Amsterdam admits environment/scalar/memory execution only;
state/account/call gas remains unsupported. A zero `Missing` count does not make
every EVM operation valid in arbitrary windows, forks or worlds.

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
CALL assumes sufficient caller gas to avoid clipping its requested amount; CREATE
uses explicit child gas budgets. Native replay never lowers the MIR caller. Live
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
Storage-array loads reserve a word-strided memory array, unpack signed integers
and left-aligned fixed bytes, check enum bounds, and allocate nested bytes objects
in order. Packing ignores dirty unused bits and reads only the declared elements;
work beyond 2048 elements is unsupported. Kernel proofs cover real builtin expansion on small arrays; native differential
tests also cover full packed words and mixed nested byte-string arrays. These
larger replay cases are not counted as Lean proofs.
External scratch frame slots use their convention-defined base of 128 and alias
ordinary memory, including across internal calls. Slices store pointer then length;
the multi-return word is the buffer-pointer slot at 32. Internal frame placement
and non-word multi-return slots remain unsupported.
Typed address calls support memory objects and slices with explicit gas for CALL,
STATICCALL and DELEGATECALL. Object headers are read before the message; slices
need no header or allocation. Returndata objects copy exactly the live buffer,
without clearing reserved padding or consuming the buffer. Implicit gas still
requires caller pricing and is unsupported.
External balance/code reads preserve address truncation and access warmth;
code introspection sees original delegation designators, not their targets.
Constant `datacopy` accepts only ranges inside known, relocation-free data, with
a 64 KiB total exported-data budget. Neighboring physical data, `datasize` of
unlinked contracts and the MIR root's unknown runtime code are not guessed.
Indirect calls, physical multi-return buffers, root runtime code
reentry/introspection and layout-dependent operations remain unsupported.

The world fixture is an explicit assumption. Declared symbolic slots, typed MIR
arguments and EVM entry stack words are universally quantified. Only EVM entries
use the trusted backend-image boundary. Audits retain this distinction and the
fixture/configuration hashes; direct MIR audits have no runtime-image hashes.
See the [world mode](README.md#storage-calls-and-creation)
for admitted forks, limits, observables and gas policies.

## Remaining work

1. Generalize fixed-world proofs to arbitrary symbolic accounts and aliases.
   Generalize direct MIR message proofs beyond explicit CREATE budgets and
   the no-clipping CALL precondition; model root reentry through MIR entry semantics.
2. Add transaction admission and EIP-7702 authorization processing, beyond
   execution of already-delegated account state.
3. Generalize calldata/returndata/program-data access and physical code layout
   from complete-entry execution to arbitrary local windows.
4. Complete MIR frames, backend allocation layouts, storage layouts and ABI helpers
   against independent semantics, rather than assuming their lowering is correct.
5. Extend dynamic gas and state execution beyond Istanbul–Osaka, including
   Amsterdam's two-resource state gas. Expand differential boundary coverage.

Unsupported semantics remain explicit. A local proof does not establish
whole-contract equivalence or an all-input correspondence theorem for the Rust
implementation.
