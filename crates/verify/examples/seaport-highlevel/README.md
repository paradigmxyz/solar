# Seaport 1.6 in high-level Solidity

A rewrite of Seaport 1.6 without inline assembly that should behave exactly
like the deployed, optimized Seaport 1.6 (ProjectOpenSea/seaport at 1170198,
seaport-core built with via-IR), except for gas. It starts from Seaport's own
reference implementation (`reference/`, MIT, see LICENSE) and changes it
wherever its observable behaviour differs: storage layout, revert data,
events, return data and edge cases. DIVERGENCES.md lists every difference
found and every intentional one.

The point is verification: specs are proved on this version, which has no
assembly and no symbolic memory, and carried to the optimized one through
equivalence.

## Layout

- `src/`: the rewrite. `lib/SeaportStorage.sol` declares all state in the
  optimized layout (`_counters` at slot 0, `_orderStatus` at 1,
  `_contractNonces` at 2, the guard at `0x929eee14`).
- `test/Deploy.sol`: deploys each variant at the same address with the same
  conduit controller, so immutables (domain separator, conduit code hash)
  agree.
- `test/Differential.sol`: runs one call against two runtimes installed in
  turn at the same address from the same state, and compares success, return
  or revert data, logs, the ordered calls and creations with their data and
  value, and the net storage changes from forge's state-diff recorder.
  `SEAPORT_B=<artifact>` compares another build instead of the rewrite,
  such as Seaport's reference, against which the regression tests fail.
- `test/*.t.sol`: differential fuzz tests and reduced regressions.
- `scripts/equiv.sh`: `forge equivalence` per function, optimized against
  the rewrite, in both guard modes. `scripts/mutants.sh`: mutants of the
  rewrite that must be reported not equivalent.

## Running

The Foundry nightly pinned in FOUNDRY_VERSION, and z3 on PATH for
`forge equivalence`:

```sh
foundryup --install nightly-5717c1b8fea92d59da287e56e8b6c798a3a36e74
scripts/fetch-seaport.sh       # Seaport 1.6 at 1170198 into lib/seaport, both builds
forge test                     # differential fuzzing and regressions
scripts/equiv.sh               # forge equivalence, Cancun deployment (tstore guard)
EQUIV_MODE=sstore scripts/equiv.sh   # deployment where TLOAD was missing
scripts/mutants.sh
```

## Status

| Function | forge equivalence (tstore, sstore) | differential fuzz | solar-verify (specs of paradigmxyz/solar#2627) |
|---|---|---|---|
| getCounter | equivalent, equivalent | passes | verified |
| getContractOffererNonce | equivalent, equivalent | passes | verified |
| getOrderStatus | equivalent, equivalent | passes | verified |
| information | equivalent, equivalent | passes | verified |
| name | equivalent, equivalent | passes | verified |
| __activateTstore | equivalent, equivalent | passes | verified |
| incrementCounter | incomplete: symbolic BLOCKHASH | passes (D1 fixed) | verified |
| getOrderHash | incomplete: symbolic keccak | passes (D2 fixed) | verified |
| cancel | incomplete: depth limit | passes (D2, D3 fixed) | unknown: 6 of 11 obligations time out in the loop |
| validate, fulfill*, match* | not yet | not yet | not yet |

The specs are applied with

```sh
python3 ../../seaport/apply_specs.py --seaport . --out OUT --search src \
  --rename Consideration=ReferenceConsideration \
  --define SSTORE_GUARD=_reentrancyGuardSstore --define TSTORE_GUARD=_reentrancyGuardTstore
```

from this directory, and checked with `solar-verify <remappings> --timeout 20000
--only ReferenceConsideration.<function> src/ReferenceConsideration.sol` in
OUT. solar-verify proofs are Z3 proofs: none is kernel-checked in Lean yet,
and none is transported to the optimized code yet. On the optimized code
the same specs verify only the three getters and `__activateTstore`; the
rest is unsupported there because of assembly.

`forge equivalence` explores both runtimes symbolically with arbitrary
storage and compares success, return or revert data, storage, logs and
balances. It does not yet show that it compares the data of calls into
unknown code (see DIVERGENCES.md, tool limits), so the differential tests
check calls separately.
