# w1: one contract through the pipeline, stage by stage

[w1.sol](w1.sol) is `contract Main { function f(uint256 a, uint256 b) external pure returns
(uint256) { return a + b; } }`, compiled with `-Ogas --evm-version osaka` (osaka is the
default). [gen.sh](gen.sh) builds solar, compiles it with `-Zemit-lean-stages=gen`, checks
that the runtime bytecode is the compiler's `--emit=bin-runtime` output with and without the
flag and the pinned 60 bytes, checks the manifest's hashes, writes [gen/Stages.lean](gen/Stages.lean)
and builds everything with lake:

```sh
solar-tv/milestone/w1/gen.sh          # from anywhere; CARGO_TARGET_DIR and CARGO_BUILD_JOBS apply
W1_SKIP_BUILD=1 SOLAR=path/to/solar solar-tv/milestone/w1/gen.sh
```

The compiler commit in the manifest (`compiler`, from `solc_compat_version`) must name
sources equal to the checkout's: gen.sh refuses uncommitted compiler changes and a stale
commit (debug builds keep the commit of the build script's last run; touch
`crates/config/build.rs` to refresh it).

## Exported files

Lean library `W1` ([lakefile.toml](../../lakefile.toml)), source directory
`solar-tv/milestone/w1`, so a file `gen/Main/X.lean` is module `gen.Main.X`. Every
definition lives in namespace `SolarTV.Export.Main` (`Main` is the contract).

| File | Definition | Type | What |
| --- | --- | --- | --- |
| `gen/Main/Stage0.lean` | `stage0` | `SolarTV.MIR.Lowered.Module` | first lowered MIR, after `lower-evm-shaped` advanced the phase |
| `gen/Main/Mir<N>.lean` | `mir<N>` | `SolarTV.MIR.Lowered.Module` | after each later MIR pass that changes it (none for w1) |
| `gen/Main/Sched.lean` | `sched` | `SolarTV.EVM.Program.Program` | runtime EVM IR as stack scheduling emits it |
| `gen/Main/Evm<N>.lean` | `evm<N>` | `SolarTV.EVM.Program.Program` | after each EVM IR pass that changes it |
| `gen/Main/Bytecode.lean` | `bytecode : List Nat`, `bytecodeHex : String` | | runtime bytecode |
| `gen/Main/Manifest.lean` | `Manifest.sources`, `compiler`, `arguments`, `evmVersion`, `optimization`, `stages`, `bytecodeHex`, `bytecodeSha256` | | what binds the stages to the compilation |

For w1 the stages are, in order ([gen/Main/manifest.json](gen/Main/manifest.json)):
`stage0` (lower-evm-shaped), `sched` (stack-schedule), `evm1` (cfg-simplify-early),
`evm2` (share-reverts), `evm3` (terminal-dedup), `evm4` (cfg-simplify-early), `bytecode`
(assemble). No lowered MIR pass and neither the backend's preparation before scheduling
nor revert legalization changes w1, so `stage0` is also the MIR scheduling compiles and
`evm4` the EVM IR assembly emits. The Stages to prove are `stage0 → sched`,
`sched → evm1 → evm2 → evm3 → evm4` and `evm4 → bytecode`.

How the artifacts map to the Lean terms:

- MIR block, value and function numbers are the compiler's `BlockId`, `ValueId` and
  `FunctionId` indices, not the renumbered ones `-Zdump=mir` prints: `@f` is function 0,
  `@entry` function 1 (`entry := 1`), and blocks are listed in index order. Pure operations
  are `.pure dst op args` with `EVM.Op` operands in EVM pop order (`shr`'s shift first);
  `ne` is `.ne dst a b`; `calldatasize`, `calldataload`, `callvalue`, `mstore` and the
  other machine reads and writes are `.effect`; `returndata off, size` is `.ret`;
  `switch` and `tail_call` are `.switch key default cases` and `.tailCall callee args`.
  Metadata (`!metadata(memory=scratch)`) is not semantics and is dropped.
- EVM IR block numbers are `BlockId`s, the layout order, not the printed labels: in `evm4`
  block 0 is `bb0`, 1 is `bb3` and 2 is `bb7`. `push bbN; jumpi` and `push bbN; jump` are
  `.jumpi N` and `.jump N` (a `jumpi` not taken continues in its block), a `jump`
  terminator is `.jump`, and a block without one falls through. Pushes are
  `.op (.stack (.push v))`, `dup n`/`swap n` are `.dup (n-1)`/`.swap (n-1)`,
  `calldatasize` is `.op (.stack .calldatasize)`, `callvalue` and `calldataload` are
  `.effect .callvalue 0` and `.effect .calldataload 1`, `mstore` is `.op .store`, and
  `return`/`revert` are `.ret`/`.revert`.

Anything outside that fragment is a compile error naming it (see
[tests/ui/codegen/lean-stages](../../../tests/ui/codegen/lean-stages)).

## Checks

[W1/Sanity.lean](W1/Sanity.lean) runs every artifact with the kernel (`decide +kernel`) on a
valid addition (returns 3), an overflow (reverts with `Panic(0x11)`), and for `stage0`,
`sched` and `evm4` a call with value, an unknown selector and short calldata (revert
empty), and checks that the bytecode is the manifest's. Two mutants show the checks tell
programs apart: `stage0` entered at `@f` returns the sum on a call with value, and `evm4`
without its panic block stops with success on an overflow. Axioms: `propext` and
`Quot.sound` only.

## Proofs

[check.sh](check.sh) is the one command for the compiler's side: it runs gen.sh, checks that
the regenerated stages are the committed ones byte for byte (the manifest may differ only in a
compiler commit with the same compiler sources), builds the proofs and runs the negative
tests:

```sh
CARGO_TARGET_DIR=... solar-tv/milestone/w1/check.sh
```

Lean library `W1Proofs` (source directory `proofs`, each module built with `lean -M 3000`),
namespace `SolarTV.Milestone.W1`:

| Module | Theorems | What |
| --- | --- | --- |
| `W1Proofs.Tactic` | `sym_exec` | kernel-only symbolic execution of two loop-free programs over a symbolic call and oracle |
| `W1Proofs.Runs.*` | `stage0_ends`, `sched_run`, `evm1_run` … `evm4_run` | the first MIR ends, and each pair of consecutive stages ends in the same result, on every call |
| `W1Proofs.Stages` | `stage_sched`, `stage_evm1` … `stage_evm4`, `w1_mir_to_evmir` | each a `Stage` of the fuel-free semantics for `anyCall` (any calldata, call value and storage), and their composition `Stage (mirSem o) (evmirSem o) stage0 evm4 anyCall` for every oracle, with no hypotheses |
| `W1Proofs.Assembly` | `walk_eq`, `decodes` | the assembly checker's walk accepts the bytecode against `itemsOf evm4`: its linear decoding is `evm4`'s instructions at the layout |

Axioms, pinned by `#guard_msgs`: `w1_mir_to_evmir` and the stages `[propext, Classical.choice, Quot.sound]`;
`evm3_run`, `evm4_run`, `walk_eq` and `decodes` `[propext, Quot.sound]`. No `sorry`, `native_decide`, `bv_decide`
or `Lean.ofReduceBool`. `sym_exec` fails closed: a program that loops, reads what the code does not
pin, or whose runs differ leaves a goal, and a leaf whose ends differ fails with
`sym_exec: the runs end in different results on this path`. The stage from `evm4` to the
bytecode under EthLib's semantics is solidity-lean's (`SolidCore.Refinement.Solar.W1.stage`).

[proofs/negative.sh](proofs/negative.sh) compiles an edited copy of one artifact in place of the
exported module and elaborates the unchanged proof against it; each must fail with its
mismatch, and unedited copies through the same harness must pass:

| Case | Edit | Rejected by |
| --- | --- | --- |
| `bytecode-add-sub` | byte 33, `ADD` (0x01) as `SUB` (0x03) | `walk_eq`: `decide` proves the walk equation false |
| `bytecode-lt-gt` | byte 36, the overflow check's `LT` as `GT` | `walk_eq`, as above |
| `evm2-jump-target` | `evm2`'s overflow branch to the empty revert | `evm2_run`: `sym_exec` reaches a path whose runs differ |
| `evm2-swapped-operands` | `evm2`'s overflow check as `lt(a, a + b)` | `evm2_run`, as above |
| `sched-add-sub` | the scheduled `add` as `sub` (what `-Zvalidate-backend-fault=opcode` corrupts) | `sched_run`, as above |
