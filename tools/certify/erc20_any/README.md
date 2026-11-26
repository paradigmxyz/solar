# UniswapV2ERC20.transfer on the any-caller route over fresh bytes

These scripts state and check one theorem about the pinned UniswapV2ERC20.transfer chain of
solidity-lean (`tools/certify/solidity-lean.rev`): the runtime bytes the pinned stage compiler
emits in this run meet `Generic.Expected` for every real caller. The proof is solidity-lean's
`CertifiedExpected.expected` and `expected_scoped` (Contracts/UniswapV2/ERC20/Certified/
TransferExpected.lean at 4332b47d, checked by digest), transported to the restated specification.

1. `run.sh LEAN_DIR SOLAR_STAGES_BIN OUT` emits the flattened port's runtime bytes with
   `Certified/uv2_erc20/options`, writes `Certified/erc20_any/{Statement,Proof,Final}.lean` with
   `gen.py`, builds them and runs `check_final.lean`.
2. `replay.sh LEAN_DIR OUT` replays every module of the closure through `leanchecker`, the
   toolchain and upstream packages included, and accounts for it with `replay.py`.
3. `neg.sh LEAN_DIR OUT` runs the mutation negatives.

Across box slots, the closure is built once. Lean's output is not byte-deterministic (rebuilding
`SolidCore.Refinement.Solar.AssembleCallWorld` on identical inputs gives one of two oleans, at any
`LEAN_NUM_THREADS`), so two slots that each build cannot have byte-identical replay plans. `slot.sh`
with `SHIP_OUT` runs `pack.sh` after the build; a later slot with `PRIOR_SHIP` and
`PRIOR_SHIP_SHA256` runs `unpack.sh` instead of building, which refuses unless every planned olean
has the earlier plan's sha256. `prep.sh` also runs `normalize_export.py`, which replaces the stage
export's temporary directory in its manifest with `EXPORT`, so independent exports are identical.

`gen.py` reads every byte, digest and stage hash from the run's files; it names no selector,
slot or byte. The statement imports no proof. The kernel does not hash the sources: `src` is
solc's AST of the port (the declared frontend), and the manifest's source digests are checked
against the files by `gen.py`. The property, context, gas and footprint negatives are harness
cases: they show the proof does not establish the mutated specification, not that it is false.
