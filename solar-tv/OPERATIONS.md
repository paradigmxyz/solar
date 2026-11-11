# Operation coverage

The compiler schemas define the scope: 107 MIR operation families and 154 EVM
opcode entries, across 15 forks. `solar-tv-replay --catalog` reflects those
schemas, including stack effects, availability and gas. Tests compare every entry
with [operations.json](operations.json), so new operations cannot go unnoticed.

| Status | MIR families | EVM opcode entries |
| --- | ---: | ---: |
| Checked within the supported fragment | 32 | 103 |
| Some forms checked | 6 | 6 |
| State model and boundary proofs, not imported | 0 | 4 |
| Message boundaries only, no child execution | 4 | 4 |
| Missing | 65 | 37 |

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

These are foundations for the full evaluator. **The CLI still rejects stateful
operations and external calls.** A child result parameter is a boundary to be
connected to real child execution, not permission to assume what a callee does.

## Remaining work

1. Connect world state to both importers and independent replay, including
   arbitrary symbolic addresses and aliases, logs, static permissions and rollback.
2. Execute nested calls and precompiles; model CREATE/CREATE2 and SELFDESTRUCT,
   including constructor execution, nonce/collision rules and fork restrictions.
3. Complete calldata/returndata/program-data access, physical code layout and
   environment/account/block operations.
4. Add MIR aggregates, slices, frames, allocation, storage layouts and ABI helpers
   against independent semantics, rather than assuming their lowering is correct.
5. Complete dynamic gas, call stipends, access-list rollback, refunds and exceptional
   outcomes for all admitted operations and forks.

Unsupported semantics remain explicit. A local proof does not establish
whole-contract equivalence or an all-input correspondence theorem for the Rust
implementation.
