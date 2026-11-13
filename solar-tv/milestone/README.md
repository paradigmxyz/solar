# Certifying a contract's backend

[certify.sh](certify.sh) turns w1's per-contract milestone into a pipeline: for any contract
the exporter (`-Zemit-lean-stages`) supports, it compiles the source, exports every stage,
writes the kernel proofs of every backend stage and checks them, with nothing written by hand
for the contract.

```sh
CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh path/to/file.sol Contract [--name NAME] [-- SOLAR_OPTIONS...]
```

It writes `milestone/NAME/`: `NAME.sol` (the source the manifest records), `options`
(default `-Ogas --evm-version osaka`), `bytecode.hex` (the pin, checked on every later run),
`gen/Contract/` (the exported stages and manifest) and `Proofs/`, the Lean library `NAME`
(added to [lakefile.toml](../lakefile.toml), each module built with `lean -M $LEAN_MEMORY`
(default 4500), one at a time), namespace `SolarTV.Milestone.<Name>`:

| Module | Theorems | What |
| --- | --- | --- |
| `NAME.Proofs.Runs.Stage0` | `stage0_ends` | the first lowered MIR ends on every call, within its block count |
| `NAME.Proofs.Runs.<Stage>` | `<stage>_run` | each pair of consecutive stages ends in the same result on every call (`sym_exec`) |
| `NAME.Proofs.Stages` | `stage_<stage>`, `NAME_mir_to_evmir` | each a `Stage` of the fuel-free semantics for `anyCall` (any calldata, call value and storage), and their composition from the first lowered MIR to the final EVM IR, for every oracle |
| `NAME.Proofs.Assembly` | `walk_eq`, `decodes` | the bytecode's linear decoding is the final EVM IR's instructions, each jumped-to block at the next `JUMPDEST`, a transfer to the next block falling through, and each jump table in the encoding its pick shows: a packed word of one-byte offsets, or entry stubs after the last block, one stride apart |

[certify.py](certify.py) writes them from the manifest: fuels are block counts (a loop-free run
visits each block at most once), times the square of one more than the call sites when the program
makes internal calls (a callee's blocks run again on every call), and the layout is the bytecode's
`JUMPDEST`s in order, a block that ends with a jump to the next block falling through as the
assembler drops that jump. These are claims until Lean checks them: a contract that loops, or a
stage that changes the meaning, leaves `sym_exec` a goal, and a wrong layout makes `walk_eq` false.
Run axioms are pinned with `#axioms_within` ([W1Proofs/Axioms.lean](w1/proofs/W1Proofs/Axioms.lean):
only `propext`, `Classical.choice` and `Quot.sound`), the stages' and the composition's with
`#print axioms`, both under `#guard_msgs`, and certify.sh audits every theorem's axioms again.
[negative.sh](negative.sh) then edits the artifacts where they have the code (the bytecode's first
`ADD`, else `LT` or `EQ`, and the first two entries of its first packed jump table; the final EVM
IR's first `add`, the key its first `sstore` pushes plus one, its call value check, its overflow
check, its first log, the signature topic and the data offset of its first log, its first
`keccak256`, the key and the mapping slot the first `keccak256` hashes (every key moved by one,
zero or not, and the mapping's slot plus one), its first `mul`, its first range test of a narrow
product, the kind of its first external call and the check of that call's success, and the first
two entries of its first jump table; the scheduled EVM IR's first `add`; in each lowered MIR pass's
output, the first branch it wrote taken the other way, else the first jump it wrote in place of a
branch sent to the branch's other target, else the first calldata it reads at the next word, else
its first constant plus one; the last MIR stage's first return of several words in reverse order
and the first further result a call binds left unbound) and requires each generated proof to fail
with its mismatch, with unedited controls passing. Once `milestone/NAME` is committed, a rerun must
regenerate it byte for byte (up to the compiler's build commit in the manifest).

w1's proofs ([w1/proofs/W1Proofs](w1/proofs/W1Proofs)) are certify.py's output too, with their
exact axiom sets pinned; [w1/check.sh](w1/check.sh) checks that they regenerate unchanged.

## Targets

- [counter](counter): `contract Counter { uint256 x; function inc(uint256 d) external { x += d; }
  function get() external view returns (uint256) { return x; } }`, 83 bytes of runtime
  bytecode, two selectors, `sload`/`sstore` and the `Panic(0x11)` overflow check. Stages
  `stage0 → sched → evm1 → … → evm5` (`cfg-simplify-early`, `block-layout`, `share-reverts`,
  `terminal-dedup`, `cfg-simplify-early`) and `evm5 → bytecode`, all proved:
  `SolarTV.Milestone.Counter.counter_mir_to_evmir`.

```sh
CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh solar-tv/milestone/counter/counter.sol Counter
```

- [masks](masks): `contract Masks { uint256 x; function inc(uint256 d) external { x += d; }
  function get() external view returns (uint256) { return x; } function low(uint8 a) external
  pure returns (uint8) { return a & 0x0f; } }`, 168 bytes of runtime bytecode, three selectors.
  Two lowered MIR passes change its code: `place-reads` reads `low`'s argument again
  where it is used (`stage0 → mir1`), and `lower-assumes` turns the fact that the masked
  argument equals itself into a branch to a revert (`mir1 → mir2`), which the stage proof
  shows is never taken. Then `mir2 → sched → evm1 → … → evm6` and `evm6 → bytecode`, all
  proved: `SolarTV.Milestone.Masks.masks_mir_to_evmir`. Its proofs need
  `sym_exec`'s word reasoning: `sym_omega` and `sym_word` read bitwise and modular
  words, with low masks as remainders, as numbers for `omega`, and `sym_agree` closes two
  ends that differ only in such words.

```sh
LEAN_MEMORY=3000 CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh solar-tv/milestone/masks/masks.sol Masks
```

- [dbl](dbl): `contract Dbl { function double(uint256 x) external pure returns (uint256) { return
  x * 2; } }`, 66 bytes. `spell-width-tests` spells the multiplication's overflow test, a
  comparison with `2^255 - 1`, as a shift by 255 tested for zero (`stage0 → mir1`), then
  `mir1 → sched → evm1 → … → evm5`, all proved: `SolarTV.Milestone.Dbl.dbl_mir_to_evmir`.
- [reorder](reorder): `contract Reorder { function take(uint256 a, uint256 b, uint256 c, uint256 d)
  external pure returns (uint256, uint256) { return (c, d); } }`, 45 bytes. `evm-inst-schedule`
  moves a calldata read to the order the stack wants it in (`stage0 → mir1`), then
  `mir1 → sched → evm1 → … → evm4`, all proved: `SolarTV.Milestone.Reorder.reorder_mir_to_evmir`.
- [rewrites](rewrites): `contract Rewrites { function f(uint256 x, uint256 y) external pure
  returns (uint256) { uint256 p = x + y; uint256 q = x + y; return (p ^ q) + (x & x) + 7; } }`,
  78 bytes, compiled with a `-Zmir-pipeline` (in [rewrites/options](rewrites/options)) that runs
  only the required lowerings to lowered MIR and then the optimizers on it, and with
  `-Zcheck-analyses=false`, as release builds of the compiler run, so they record no facts. Each
  of the four lowered MIR passes makes an optimizing rewrite: `cse` merges the second `x + y` and
  its overflow check into the first (`stage0 → mir1`), `egraph` rewrites `p ^ p` to zero and
  `x & x` to `x` and folds the overflow check of their sum to false (`mir1 → mir2`), `sccp`
  turns that constant branch into a jump (`mir2 → mir3`), and `check-elim` drops the duplicate
  overflow check the first one dominates (`mir3 → mir4`). Then `mir4 → sched → evm1 → … → evm4`,
  all proved: `SolarTV.Milestone.Rewrites.rewrites_mir_to_evmir`. The e-graph stage needs
  `sym_omega` and `sym_word` to read a word combined with itself (`x ^ x`, `x & x`, `x | x`) and
  a sum with zero as the word it leaves.

```sh
LEAN_MEMORY=3000 CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh solar-tv/milestone/rewrites/rewrites.sol Rewrites
```

- [folds](folds): `contract Folds { function g(uint256 x, uint256 y) external pure returns (uint256)
  { uint256 k = 10; uint256 t = x; if (k > 5) { t = y - x; } return t; } }`, 68 bytes, with the
  same kind of `-Zmir-pipeline` ([folds/options](folds/options)) running `const-fold`,
  `branch-simplify`, `dce` and `cfg-simplify` on lowered MIR. `const-fold` folds `10 > 5` to true
  (`stage0 → mir1`), `branch-simplify` turns that branch into a jump and drops the arm it never takes
  and its phi input (`mir1 → mir2`), and `cfg-simplify` merges the blocks of the jump chain left and
  the phi with one input (`mir2 → mir3`); `dce` finds nothing left. Then
  `mir3 → sched → evm1 → … → evm4`, all proved: `SolarTV.Milestone.Folds.folds_mir_to_evmir`.

- [bank](bank/bank.sol): a `mapping(address => uint256)` of balances and a total, credited by
  an internal function that `deposit(uint256)` (for `msg.sender`, emitting the indexed
  `Deposit` event) and `depositFor(address,uint256)` call, and `balanceOf(address)`; 454 bytes
  of runtime bytecode with `keccak256` mapping slots, `log2`, ABI address checks and internal
  calls through pushed return labels. Stages `stage0 → mir1 → … → mir4 → sched → evm1 → … →
  evm11` (`place-reads`, `spell-width-tests`, `lower-assumes`, `evm-inst-schedule`,
  `stack-schedule`, then `peephole-early` to `stack-reschedule`) and `evm11 → bytecode`, all
  proved: `SolarTV.Milestone.Bank.bank_mir_to_evmir`, for every calldata (so every mapping key),
  value, storage and oracle (so every caller and every hash function). solidity-lean proves
  `evm11 → bytecode` with its generic assembler check, which follows the return labels, the
  hashes and the log (`SolidCore.Refinement.Solar.Bank.backend`, under an oracle whose caller and
  hash are EthLib's).

```sh
CARGO_TARGET_DIR=... LEAN_MEMORY=3000 solar-tv/milestone/certify.sh solar-tv/milestone/bank/bank.sol Bank
```

- [events](events/events.sol): `contract Events { event Transfer(address indexed from, uint256
  indexed id, uint256 value); function transfer(uint256 id, uint256 value) external { emit
  Transfer(msg.sender, id, value); } }`, 130 bytes: one `log3` whose topics are the event's
  signature, the caller and the id and whose data is the value's word at offset 0. Stages
  `stage0 → mir1 → sched → evm1 → … → evm5` (`lower-assumes`, the caller's address check, then
  `stack-schedule` to `cfg-simplify-early`) and `evm5 → bytecode`, all proved:
  `SolarTV.Milestone.Events.events_mir_to_evmir`. Its negatives change the log's signature topic
  and its data offset, and drop the log; solidity-lean proves `evm5 → bytecode` generically
  (`SolidCore.Refinement.Solar.Events.backend`).

```sh
CARGO_TARGET_DIR=... LEAN_MEMORY=3000 solar-tv/milestone/certify.sh solar-tv/milestone/events/events.sol Events
```

- [c4](c4): `contract Main { uint8 v; uint8 w = 7; function add(uint8 x) external returns (uint8) {
  v += x; w ^= x; return v; } }`, 107 bytes, compiled with solar's default options and
  `-Zcheck-analyses=false` ([c4/options](c4/options)). Both fields are packed in slot 0: the code
  masks `v` out of the slot, adds with the `uint8` overflow check, merges it back, reads `w` with
  `BYTE`, XORs, shifts and merges it, and stores the slot. `place-reads` reads the argument again
  where it is used (`stage0 → mir1`) and `evm-inst-schedule` moves a mask (`mir1 → mir2`); the stack
  scheduler commutes the operands of AND, OR, XOR and ADD, which `sym_word` closes by `ac_rfl`
  (`mir2 → sched`), and `compact-pushes` spells the masks `~0xff` and `~0xff00` as `push; not`,
  which `sym_eval` computes back to the literal (`evmNotLit`, `evm1 → evm2`). Then `evm2 → … →
  evm5`, all proved on every call (any call value, calldata and storage):
  `SolarTV.Milestone.C4.c4_mir_to_evmir`, and the assembly walk of `evm5` against the bytecode.
  solidity-lean's `SolidCore.Refinement.Solar.C4.stage` (branch ck-asm2, `scripts/refinement/c4.sh`)
  proves `evm5` to the bytecode under EthLib's semantics, from any storage at zero call value.

```sh
LEAN_MEMORY=3000 CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh solar-tv/milestone/c4/c4.sol Main --name c4 -- -Zcheck-analyses=false
```

- [mulx](mulx): `contract Main { uint8 x8; uint16 y16 = 3; uint256 z; function m(uint8 a, uint16 b)
  external { x8 *= a; y16 *= b; } function n(uint8 a, uint16 b, uint256 c) external returns
  (uint256) { z = uint256(a) * b * c; return z; } }`, 186 bytes, with `-Zcheck-analyses=false`
  ([mulx/options](mulx/options)). Checked narrow multiplications into two fields packed in slot 0
  (one range test of each exact product, `push 255; dup; gt` and a shift by 16, joined by `or`),
  and a `uint256` product whose overflow test is `c == 0 || p / c == ab` (MUL and DIV). Stages
  `stage0 → mir1 → mir2 → mir3 → sched → evm1 → … → evm8` (`place-reads`, `spell-width-tests`,
  `evm-inst-schedule`, `stack-schedule`, `cfg-simplify-early`, `compact-pushes`, `block-layout`,
  `terminal-dedup`, `cfg-simplify-early`, `block-layout`, `stack-reschedule`, `terminal-layout`),
  all proved on every call (any call value, calldata and storage):
  `SolarTV.Milestone.Mulx.mulx_mir_to_evmir`. The stack scheduler commutes the operands of the
  products and masks in the tests too, so two paths' cases contradict only up to that order, which
  `sym_comm_contra` closes (`mir3 → sched`); and `terminal-layout` puts `m`'s STOP last, which the
  assembler leaves out (the code ends there), as `W1Proofs.Items` now spells it. solidity-lean's
  `SolidCore.Refinement.Solar.MulX.certified` (branch ck-mulx-sl) composes these stages with its
  source stage and its assembly stage into one theorem from SolidCore's `Main` to the bytecode.

```sh
LEAN_MEMORY=3000 CARGO_TARGET_DIR=... solar-tv/milestone/certify.sh solar-tv/milestone/mulx/mulx.sol Main --name mulx
```

- [reader](reader/reader.sol): `Reader.read(IOracle)` returns `o.price() + 1` from an external
  `STATICCALL`: the ABI address check, the call with the gas left, the forwarding of a failed
  call's return data with `RETURNDATACOPY` and `REVERT`, the check that the return data
  holds a word, and the `Panic(0x11)` overflow check; 113 bytes of runtime bytecode. Stages
  `stage0 → mir1 → mir2 → mir3 → sched → evm1 → … → evm7` (`place-reads`, `spell-width-tests`,
  `evm-inst-schedule`, `stack-schedule`, then `cfg-simplify-early` to `cfg-simplify-early`)
  and `evm7 → bytecode`, all proved for every oracle, the call's result and return data
  among it: `SolarTV.Milestone.Reader.reader_mir_to_evmir`. Its negative tests also make the
  call a `DELEGATECALL` and drop the check of its success.

```sh
CARGO_TARGET_DIR=... LEAN_MEMORY=3000 solar-tv/milestone/certify.sh solar-tv/milestone/reader/reader.sol Reader
```

- [pair](pair/pair.sol): `split(uint256)` returns two words, the new total it stores and its
  input XORed with it, from an internal function that `add(uint256)` calls once and
  `addTwice(uint256)` twice, the second call on the first one's second result; 117 bytes of
  runtime bytecode. Stack scheduling returns both words on the stack and elides MIR's reads of
  the multi-return buffer, which the MIR stages bind at the call. Stages `stage0 → sched → evm1
  → evm2` (`stack-schedule`, `cfg-simplify-early`, `block-layout`) and `evm2 → bytecode`, all
  proved: `SolarTV.Milestone.Pair.pair_mir_to_evmir`. Its negative tests also return the two
  words in reverse order and leave the second result of the first call unbound.

```sh
CARGO_TARGET_DIR=... LEAN_MEMORY=3000 solar-tv/milestone/certify.sh solar-tv/milestone/pair/pair.sol Pair
```

- [switch](switch/switch.sol): `Switch.select(uint256)` stores one of eight values by an assembly
  `switch` on the key, compiled to a jump table: the code masks the key to its bucket (`and 7`) and
  jumps through a packed word of eight one-byte offsets (`PUSH1 24; ADD; PUSH8; SWAP1; BYTE;
  JUMP`), each target checking the key before its store; 167 bytes of runtime bytecode. Stages
  `stage0 → sched → evm1 → … → evm8` (`stack-schedule`, `cfg-simplify-early`, `block-layout`,
  `share-reverts`, `terminal-dedup`, `cfg-simplify-early`, `inline-returns`, `cfg-simplify`,
  `loop-layout`) and `evm8 → bytecode`, all proved on every call:
  `SolarTV.Milestone.Switch.switch_mir_to_evmir`. MIR's `switch` tests the key against each case;
  where no case fixes the key, `sym_exec` splits the table's lookup into one test of the index per
  entry (`split_table`, `W1Proofs.Tactic.table_lookup`). Its negative tests also swap the first
  two entries of the table, in the final EVM IR and in the bytecode.

```sh
CARGO_TARGET_DIR=... LEAN_MEMORY=3000 solar-tv/milestone/certify.sh solar-tv/milestone/switch/switch.sol Switch
```

## Limits

The scope is the exporter's fragment (anything else is a compile error naming it) and loop-free
programs. The stage from the final EVM IR to the bytecode under EthLib's semantics is proved per
contract in solidity-lean; here the walk only shows the decoding. The caller is the oracle's, any
160-bit address, so a contract that reads `msg.sender`, such as bank, is proved for every caller.
Hashes are an oracle function of the hashed bytes and the proofs hold for every oracle, KECCAK256
among them, but say nothing about collisions. Of jump tables, the assembly walk spells the packed
one-byte table and the stride table, the encodings `-Ogas` builds; a table relative to a base or
packed wider than a byte (the size objective, `-Zpack-wide-jump-tables`) makes certify.py stop with
an error naming the jump tables it cannot spell, and the walk refuses it. Two milestone directories
whose contracts share a name export the same `SolarTV.Export.<Contract>` namespace and cannot be
imported together.

## Loop invariants from solar-verify

A spec proof of a function that loops (solidity-lean's Contracts/VerifyCounter/Bump.lean, bump's
loop of crates/verify/examples/Counter.sol) goes through the loop's header in the first lowered
MIR, and needs an invariant there. None is written by hand: [inferred.py](inferred.py) carries the
invariants and the measure that `solar-verify --show-inferred` infers for the source loop to the
header, binding the loop variable and its bound to the operands of the header's `lt` branch, a
state variable to the phi whose entry input loads its slot and a parameter to its calldata word,
and adds the frame the compiler keeps around the loop from the MIR alone (storage unchanged, the
definitions of the values the loop reads, the branches before it, the facts the header checks of
memory). The proof checks every conjunct on every segment, so a wrong clause or binding fails by
mismatch. [verify_counter/inferred.sh](verify_counter/inferred.sh) writes Counter's:

```sh
solar-tv/milestone/verify_counter/inferred.sh target/release/solar-verify     $SOLIDITY_LEAN/Certified/verify_counter/gen/Counter/Stage0.lean     $SOLIDITY_LEAN/Contracts/VerifyCounter/Inferred.lean
```

Arrays and quantifiers: for a library function over a `uint256[] memory` parameter inlined into
its caller (ArrayMax.max of crates/verify/examples/ArrayMax.sol, behind `ArrayMaxEntry.max`), the
array is the memory array whose length word the header checks equals the loop's bound, `xs[e]`
its word at `xs + 32 + 32 * e`, and `forall`/`exists` clauses over a `uint256` become quantifiers
over words. The function's one local is the header's one remaining phi; a header phi that steps by
a constant whenever the loop variable steps by one is an induction pointer, related to the loop
variable; a load the loop's entry block makes after its last memory write reads the memory the
loop sees; and a `calldatacopy` before the loop that nothing later overwrites contributes its
words as the calldata's. [arraymax/inferred.sh](arraymax/inferred.sh) writes ArrayMaxEntry's.

## Loops: cut points

[certify.sh](certify.sh) certifies a contract whose backend loops through cut points, where the
symbolic runs above cannot end; [certify-loops.sh](certify-loops.sh) runs the cut-point proofs alone. For each pair of consecutive stages,
[cutpoints.py](cutpoints.py) picks cut points that cut every cycle of both programs and the
relation between their configurations there, from what the compiler kept across the pass, and
writes the obligation of `SolarTV.CutPoint`: from every related pair, the two loop-free segments
end alike. `sym_seg` ([W1Proofs/Segment.lean](https://github.com/paradigmxyz/solidity-lean)) proves each one
by symbolic execution from configurations of variables, and `LayerSim` lifts them to the stage
in both directions, for any number of iterations. No relation is written by hand:

- MIR to MIR: the loop headers by block number, the same state, every value live into the header
  the same word, each phi's input from one predecessor the other's from its own, and the facts
  (assume) the source checks from the header before it writes memory or storage.
- MIR to the scheduled EVM IR: the header's scheduled block and the stack the scheduler recorded
  there (`Schedule.json`, which `-Zemit-lean-stages` writes from the scheduler's own map), each
  value's word and a phi's word as its input from the predecessor.
- EVM IR to EVM IR: blocks of the same number that both programs loop through, the same words at
  the same height, and the same state.

A pair the procedure cannot relate fails closed with the pass and the reason, such as a loop in
a function that an internal call enters. A wrong relation fails a segment by mismatch, and so
does running out of a segment's fuel, which matches nothing.

- [sqrt](sqrt/sqrt.sol): Uniswap v2's `Math.sqrt` (the Babylonian loop) behind an external
  entry point, 176 bytes at `-Ogas`. Stages `stage0 → mir1 → sched → evm1 → … → evm12`
  (`lower-assumes`, `stack-schedule`, then `cfg-simplify-early` to `cfg-simplify`, among
  them `copy-loop-blocks`), all proved on every call and oracle however many times the loop
  runs: `SolarTV.Milestone.Sqrt.sqrt_mir_to_evmir`, and each stage backwards (`<stage>_back`).
  [cutpoint-negative.sh](cutpoint-negative.sh) checks that halving by four instead of two in the
  loop body, and inverting its exit test, each fail the last stage by mismatch.

- [uv2_sqrt](uv2_sqrt/uv2_sqrt.sol): `Math.sol` exactly as `solar-tv/uniswap-v2/port.py` writes it from
  Uniswap/v2-core@6a9e7c9 (`min` and `sqrt`), then the same entry point; the same bytecode as
  [sqrt](sqrt/sqrt.sol), proved the same way: `SolarTV.Milestone.Uv2Sqrt.uv2_sqrt_mir_to_evmir`, with
  the same two mutants failing by mismatch. In `UniswapV2Pair` itself `sqrt` is an internal
  function that `mint` calls, which the procedure does not relate yet: every stage of Pair fails
  closed with "a loop in @sqrt, which an internal call enters".

```sh
SOLAR=target/debug/solar solar-tv/milestone/certify-loops.sh sqrt SqrtEntry
```
