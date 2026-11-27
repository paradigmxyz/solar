# Certifying a contract

`scripts/certify FILE.sol` proves in Lean, checked by the kernel, that solar's runtime
bytecode for a contract does what its first lowered MIR does, and, when solidity-lean states one,
that the contract's source or solar-verify spec holds of that bytecode. Nothing is written by
hand per contract, and the command fails closed: any step that does not check stops it with an
error, and only a successful run with an end-to-end theorem prints its `CERTIFIED` line.

```sh
scripts/certify tools/certify/examples/counter.sol
```

All Lean lives in [paradigmxyz/solidity-lean](https://github.com/paradigmxyz/solidity-lean)
(EvmYul, SolidCore, SolarTV, Loom, Verify and the generated per-contract `Certified/`). This repository
holds no `.lean` files of its own for certification: [solidity-lean.rev](solidity-lean.rev)
pins the solidity-lean commit the generated proofs are checked against, and the driver checks it
out in `target/certify/solidity-lean` (or uses `SOLIDITY_LEAN_DIR`, which must be at that
commit).

What a run does:

1. Checks out the pinned solidity-lean.
2. Builds solar from this checkout (or takes `SOLAR`, which must report a commit with this
   checkout's compiler sources).
3. Compiles `Certified/NAME/NAME.sol` with `-Zemit-lean-stages`, which exports every
   backend stage as Lean, and checks that the flag does not change the bytecode, that the
   bytecode is the pinned `Certified/NAME/bytecode.hex` when there is one, and the manifest's
   hashes.
4. [certify.py](certify.py) writes `Certified/NAME/Proofs`: a `sym_exec` proof per pair of
   consecutive stages, their composition from the first lowered MIR to the final EVM IR, and the
   assembly walk of the bytecode against the final EVM IR.
   When the final EVM IR is inside the fragment the assembly checker executes, it also writes
   `Proofs/Execution.lean`: the checker of code with loops (`AssembleLoop.checkL`) accepts the
   bytecode on a certificate [asm_cert.py](asm_cert.py) infers (the kernel's evaluation re-derives
   every claim, so the inference is not trusted), and `NAME_bytecode` composes it with the
   backend stages read backwards: at every gas, every result of the exact bytecode is the first
   lowered MIR's or a failure that changes nothing. Outside the fragment (external calls, jump
   tables) the bytecode is only decoded, and the driver says so.
5. Builds them with lake, one module at a time, and audits every theorem: only `propext`,
   `Classical.choice` and `Quot.sound`, and no `sorry`, `native_decide`,
   `bv_decide`, `ofReduceBool`, axiom or extern in the generated files.
6. solar-verify (`SOLAR_VERIFY`, else built from `crates/verify`) exports the contract's
   verification conditions with `--dry-run --dump-lean`. Each function whose conditions fall
   outside the fragment solidity-lean checks is printed as `REFUSED (unproved)` and recorded so in
   the run's record, never as passing; `--require-spec` fails on any.
   When solidity-lean has `Certified/NAME/headline` (lines `MODULE THEOREM`, the theorems
   that state the source or spec of NAME against its bytecode), builds and audits those too.
   `--require-headline` fails without one.
7. [negative.sh](negative.sh) edits the exported artifacts where they have the code (an opcode
   of the bytecode, a constant, a branch, a check, a log, a hash key...) and requires each
   generated proof to fail with its mismatch, not a timeout, with unedited controls passing.
Every successful run reports, and writes to its record (`Certified/NAME/record.json` and
`.lake/certify-NAME.json` in the checkout), separate statuses rather than one claim:

- `mir_to_evmir`: the composition of the stage proofs, from the first lowered MIR to the final
  EVM IR.
- `assembly`: `decoded only` (the bytecode decodes to the final EVM IR's instructions) or
  `executed` (a theorem that running the bytecode refines the final EVM IR).
- `vc`: solar-verify's conditions exported, refused per function, or, with `--require-spec`,
  built and discharged by the theorem `Certified/NAME/spec` names (lines `MODULE THEOREM`),
  which composes them with the bytecode.
- `end_to_end`: a headline or spec theorem from the source or spec to the exact bytecode, with
  its full statement (`end_to_end_statement`) and its binders and premises (`hypotheses`).
- `refused`, `vacuous` (not checked yet) and `caveats`.

A run prints `CERTIFIED` only when equivalence, termination, the spec and resources are all
proved, which no run does yet; the record's `statuses` reports each of them apart, and the run prints
`CHECKED ...` with what it proved.

[regenerated.sh](regenerated.sh) is a regression check, run as its own CI step and not part of
certification: when solidity-lean commits `Certified/NAME`, the files a run regenerated must be
the committed ones, up to the compiler's commit in the manifest.

[checkout.sh](checkout.sh) prints the path of the pinned checkout, fetching it when needed, for
other tools that need solar's Lean (such as the translation validator).

[test.sh](test.sh) checks that the driver refuses what it must before any Lean runs: a
solidity-lean checkout at another commit, a solar built from other sources, a file with several
contracts, a bytecode that is not the pin, and a source that does not compile.

CI's `certify Counter` job runs test.sh and then the driver on
[examples/counter.sol](examples/counter.sol), negative tests included.

To move the pin, update [solidity-lean.rev](solidity-lean.rev) to a solidity-lean commit and
run the driver on Counter.

## The generic driver's supported subset

[generic.py](generic.py) (`tools/certify/certify SOURCE.sol SPEC`) takes any contract inside a
stated subset of Solidity and refuses everything else before any build, with exit 3 and a
`BLOCKED: <feature> (...)` line naming the first place each feature appears; the record's
`status` is `BLOCKED` and its `blocked` list holds every refusal. A `BLOCKED` run proves
and claims nothing. The subset is one self-contained file and one concrete contract in it that has
no bases and no fallback or receive function (constructor parameters: see below). The file may hold other
contracts, interfaces and libraries: with several concrete contracts, `--contract` names the one
certified, and the statement is about it alone (its source is the whole unit with that contract
named, its bytes that contract's export). A sibling is part of the certified contract only as the
library internal functions and free functions it calls, which solar inlines and the envelope
checks as the contract's own code; a public library function it calls runs by DELEGATECALL and is
an external call. The contract and the code it inlines have none of:

- imports;
- inline assembly;
- external calls (including try/catch, low-level calls, calls of external functions and of public
  library functions), contract creation (`new`, and `type(C).creationCode` or `runtimeCode`,
  which embed another contract's bytecode) and payable functions or value transfers;
- reads of other accounts (`code`, `codehash`, `balance`) and of `selfdestruct`, `gasleft` or
  `blockhash`;
- loops, apart from the counted `for` loops the source erasure covers (one per external function,
  below), which still block until the published statement applies their join;
- recursion: a function or modifier that can reach itself through internal calls or modifier
  invocations (directly or through others), and calls through internal function values, whose
  callee the source does not name. The generic stage proofs need a cut point at every back edge,
  and a recursive call site has none. [RECURSION.md](RECURSION.md) sketches what supporting it
  would take.

A natspec spec may in addition read only `msg.value` from the environment. Nothing in the subset
is selected by contract name or hash.

### Constructor parameters

A constructor whose parameters are all static `uintN` or `address` words is inside the subset on
the compiled tier (no specification): the published statement holds for every argument list
(`∀ (A0 A1 … : Nat), A0 < 2 ^ W0 → …`), against the source deployed with those words
(`solidCoreFromArgs callerAddress fuel [A0, …]`), from any storage covering the slots that deployment
writes. The probe prints the erasure of the constructor's body over the argument words, and the
Lowering module checks it in the kernel (`Generic.ctorShapeOf`, solidity-lean
`Verify/Generic/CtorWords.lean`); its leaves must be normal with one set of writes and no logs, or
revert. When some leaf reverts (a checked operation that may overflow, a `require`), the statement
has the premise `Generic.CtorReturns fuel contract? [A0, …]` (SolidCore's constructor returns on
those words) and states one witness list on which it returns. Refused, each with its reason:
dynamic and multi-word arguments (`bytes`, `string`, arrays, structs), the word types not bound yet
(`bool`, `intN`, `bytesN`, enums, contract and function types), immutables (the runtime bytes
depend on the arguments), a specification (SpecConnect and the Expected statement run the
constructor with no arguments), and a constructor body the erasure does not cover (for one, a write
to a storage field other than an unsigned scalar or mapping). The creation bytecode is not part of
the statement, as for a constructor without parameters; the lifecycle tier
(`--tier lifecycle --constructor-args`) states it for given words.

### Loop cut-point certificates

[loop_equiv.py](loop_equiv.py) writes a `Verify.LoopSource.Equiv.Cut` certificate for one
function's loop from solar-verify's loop-head manifest (format `solar-verify loop heads 1`) and
the `Stage0.lean` that `-Zemit-lean-stages` exports. A certificate is only an input to
`Cut.check`, a kernel check in solidity-lean that rejects it unless every arrival at the loop
header matches it; it is never a reason to pass. [test_loop_equiv.py](test_loop_equiv.py) pins its
reading of the MIR export on a real one (`testdata/loop_equiv/`) and runs without Lean.

The envelope reads each loop's shape from solc's AST (`loop_cuts.loop_shapes`). A loop the source
erasure (solidity-lean's `eraseLoopStmt`) certainly does not cover is blocked with the reason:
while and do-while loops, nested loops, several loops in one function, `break`, `continue`,
`return`, `revert`, `require` or `assert` in the loop, logs, decrements, `delete`, writes to
mapping or array elements, `++` on a narrow word or an indexed expression, a condition other than
`counter < bound`, and loops in modifiers, constructors, fallback, receive or internal functions.
Any other `for` loop is blocked as a counted loop whose source premises the hook generates; the
erasure itself, in the kernel, decides the rest.

generic.py calls it from [loop_cuts.py](loop_cuts.py), beside `loop_resources`, when the
envelope blocks a source on a loop. A function refused by shape stops at step `shape`, before any
export or build. For each other function solar-verify's manifest
(`--emit-loop-heads`, then `--check-loop-heads` on the file; z3 from `SOLAR_VERIFY_Z3`) gives a
loop, it writes `LoopCut.<function>.lean` into the work directory from the run's own
`Stage0.lean`, with the selector and argument count from solc's AST. In a clone of the base it
then runs the base's obligation generator (`Verify.LoopSource.Equiv.GenObl`) and builds the
`Obl` module it writes. With no `--loop-summary` (the default) the generator takes the erasure
route: it erases the imported function's body with `eraseLoopStmt` at the statement fuel
`2 ^ 256 + 8`, refuses a body outside the erasure's shape, and the `Obl` module proves, besides the
leaves of the cut check, the tree, the sequents and the obligations, `erased` (the function's
arity, its erasure and the erasure's tree, `GenObl.Erased`; `Erased.runs` gives its `BodyRuns` by
`bodyRuns_of_eraseLoop`) and `same` (the erased tree and the simplified tree the sequents are
stated over observe alike): the source and obligation premises of solidity-lean's
`EraseLoopJoin.GeneratedTarget`. The record's `loop_cuts` holds, for each function every step
completed for, the manifest, the certificate and the obligation modules with their digests, the
measure and the route. `loop_cuts_blocked` names the step each other function stopped at. A base
without `Cut.lean` or the generator (the current pin has neither) stops there, before any build.
[test_loop_cuts.py](test_loop_cuts.py) pins each way a step can fail and runs without Lean.

When every loop function's obligations are built, [loop_final.py](loop_final.py) publishes the
loop function's final theorem in the same clone: the backend stage proofs of the run's export
(certify.py, through cut points), [loop_resources.py](loop_resources.py)'s fixed-fuel module on the
run's bytes, `LoopJoin.join` (solidity-lean's `EraseLoopFinal.final_of_generated`, hence
`EraseLoopJoin.eraseLoop_certified_generated`), and a `shape.json` for final.py with
`iteration_bound` (the loop's limit bounded by `K ≤ Kmax`) and `source_ast`. It builds
`NAME.Final` and checks it with check_final.lean and the run's bytes. The statement covers the loop
function's calls from any storage (runtime only), with the specification `True` and its vacuity
declared, so the run stays `BLOCKED`; the record's `loop_final` holds the checked statement or
the step that refused (`route`, `functions`, `abi`, `resources`, `export`, `stages`,
`statement`, `build`, `check`). [test_loop_final.py](test_loop_final.py) runs without Lean.

## The published statement

A contract with `Certified/NAME/shape.json` gets one published theorem,
`Certified.NAME.Final.final` in `Certified/NAME/Final.lean`, which [final.py](final.py) writes:
its statement `Statement` is fixed by the driver from `shape.json` (the checked source, or a source
semantics and program below it, the declared valid calls, the allowed statuses, the specification,
named environment assumptions and the gas bound) and from the runtime bytecode this run compiled,
never from a proof module. The statement binds the freshly exported bytecode to those bytes by
exact equality, decided by the kernel, and states one of solidity-lean's shapes of it:
`Shapes.Certified` (equivalence at every gas, termination with an allowed status, the
specification, and resources: the source's result from a proved gas bound on),
`Shapes.Functional` (the first three) or `Shapes.Compiled` (equivalence with the gas bound,
termination and resources, for a contract with no specification: never the strongest status). The
proof module named in `shape.json` supplies only a term the kernel must accept at that type, and
the theorem's axioms are audited (only `propext`, `Classical.choice` and `Quot.sound`).

For a contract whose source stage is the generic check, `shape.json`'s `generic` block makes
[shape_gen.py](shape_gen.py) write that proof module, `Certified/NAME/Shape.lean`, from this run's
export: it applies `Shapes.compiled` (or `compiled0`) to the source check, the backend stages, the
last stage's fuel and the loop checker on a certificate [asm_cert.py](asm_cert.py) infers, adds
the triples a specified contract names, and proves that the checker rejects the bytecode with a
jump target moved, an SSTORE dropped and an unsupported opcode.

The statement is its own module, `NAME.Statement`, which never imports the proof module, and
`NAME.Final` only proves it. [check_final.lean](check_final.lean), which imports nothing but Lean,
reads the built environments and requires `final`'s type to be exactly `NAME.Statement`'s
`Statement`, that module not to import the proof module, and `final`'s axioms to be `propext`,
`Classical.choice` and `Quot.sound`: no name, alias, notation or macro of the proof module changes
what is stated. The strongest status needs the specification from solar-verify's export of the
run's source (`spec_export`: the driver writes it as `NAME.gen.C.Spec` and the statement reads it
with `Shapes.bundlePost`) and a non-vacuity witness in the statement (`NonVacuous`: a valid call
with a result the specification rejects, and one with a revert the allowed statuses reject). A
statement with a declared `vacuity` is recorded below the strongest status. When `shape.json`
names the checked source AST (`source_ast`), [source_bind.sh](source_bind.sh) regenerates it from
the run's source with the pinned solc and fails on any difference.

The record's statuses (`equivalence`, `termination`, `spec`, `resources`, `vacuity`, `source`,
`environment`, `assumptions`, `excluded`, `strongest`) come from that statement; `CERTIFIED` is
printed only for the strongest. [final_neg.sh](final_neg.sh) checks that variants fail: stale or
substituted bytecode, the strongest status without resources, a wrong gas bound, a wrong
postcondition, no allowed status, a dropped environment premise, a `sorryAx` proof, vacuous
specifications (with no witness, a `sorryAx` witness, or a proof the kernel accepts), and a proof
module that declares the statement's names, aliases its name or rewrites its command by a macro.

The kernel replays the published theorem's whole constant closure in every run, with nothing skipped
for an earlier one ([replay_closure.lean](replay_closure.lean)): every constant its type and proof
reach, through types and values, inductive blocks, constructors and recursors, the toolchain's and
the upstream lake packages' (Mathlib, Batteries, Aesop and the rest) included, from an empty
environment. A closure with an unsafe or partial constant or an axiom beyond `propext`,
`Classical.choice` and `Quot.sound` fails, and so does a theorem whose type is not exactly the
statement. `CERTIFY_REPLAY_SHARDS=N` splits the proofs of the closure's theorems into N shards that
run at once. The proof builds in a mount namespace where all of HOME is read-only but the package's
.lake, and what later steps read is fingerprinted before and after it; a toolchain or package
altered before the run is not detected.

The replay is accounted for ([replay.py](replay.py) `closure`): every shard exits 0 on the same
closure and their checked proofs are an exact disjoint union of its theorems, or the run fails. The
receipt (the closure's count, digest, kinds and constants, its axioms, each shard, and the checker and
tool with their digests) is the record's `replay` and its status `statuses.replay`.
[replay_closure_test.sh](replay_closure_test.sh) tests it: a tampered Init lemma, a `sorry` or an
axiom deep in the closure, an omitted dependency, omitted, overlapping or incomplete shards and a
theorem whose type is not the statement all fail.

The driver checks every producer's exit status: a list read from a command goes through `capture`
([inputs.sh](inputs.sh)), never `mapfile < <(...)` or a pipe into `while read`, both of which
discard it, so a producer that prints plausible output and then fails stops the run. The run's
inputs are checked before use (the solar options, the loops file, the runtime bytecode and its pin,
solar-verify's exports with [export_check.py](export_check.py), the published statement's record),
and [test.sh](test.sh) has a negative case for each.

A run in a memory-limited cgroup scope is admitted and accounted for by
[box_admit.sh](box_admit.sh): `admit` reads back the scope's memory.max (a byte count, never
`max`) and memory.swap.max (0) and checks the inner process is in the scope, `pidtree` checks in
one sample that every descendant of the inner process is in it, and `receipts` checks a finished
run's directory has every receipt (record, binding, replay receipt, axioms, negative tests)
with the digests its receipts.sha256 lists. Each aborts on anything it cannot read or that
differs; [box_admit_test.sh](box_admit_test.sh) runs each failure on fixture cgroup and /proc
trees.
