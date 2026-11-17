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

`gen.py` reads every byte, digest and stage hash from the run's files; it names no selector,
slot or byte. The statement imports no proof. The kernel does not hash the sources: `src` is
solc's AST of the port (the declared frontend), and the manifest's source digests are checked
against the files by `gen.py`. The property, context, gas and footprint negatives are harness
cases: they show the proof does not establish the mutated specification, not that it is false.
