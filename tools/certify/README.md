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
   \`Proofs/Execution.lean\`: the checker of code with loops (\`AssembleLoop.checkL\`) accepts the
   bytecode on a certificate [asm_cert.py](asm_cert.py) infers (the kernel's evaluation re-derives
   every claim, so the inference is not trusted), and \`NAME_bytecode\` composes it with the
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

Only a run with an end-to-end theorem prints `CERTIFIED`; one without prints `CHECKED ..., not
end to end`.

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
