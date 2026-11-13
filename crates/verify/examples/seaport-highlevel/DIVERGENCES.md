# Differences from the optimized Seaport 1.6

## Found and fixed in the rewrite

Each was a difference between Seaport's `reference/` and the optimized
contracts. Fixed differences found by fuzzing or `forge equivalence`
have a test in test/Regressions.t.sol or test/OrderHash.t.sol, which
passes on the rewrite and fails with `SEAPORT_B` set to Seaport's
reference build.

- R1 `name()` returned "Consideration"; it returns "Seaport". Found by
  `forge equivalence` on `name()`.
- R2 The EIP-712 version was "1.6-reference"; it is "1.6", so `information()`
  and every order's signed digest match. Found on `information()`.
- R3 Storage layout. The reference kept a bulk-order typehash mapping, a
  storage reentrancy guard and two basic-order lookup mappings ahead of
  `_counters`, `_orderStatus` and `_contractNonces`. They are now at
  slots 0, 1 and 2, the typehash and basic-order lookups are pure functions,
  and the guard is the optimized one. Found on `getCounter`.
- R4 Reentrancy guard. The reference used a storage guard everywhere; the
  rewrite follows seaport-core's: TSTORE when the EVM supported it at
  deployment, else SSTORE at `0x929eee14` until `__activateTstore` (now
  present) switches to TSTORE, with the same checks, writes and errors in
  the same order.
- R5 `incrementCounter` now rejects calls made while the guard is held, as
  the optimized code does.
- D1 `incrementCounter` panicked (0x11) when the counter plus
  `blockhash(block.number - 1) >> 128` exceeded 2^256 - 1; the optimized
  code wraps. Found by `testFuzz_incrementCounter`.
- D2 `getOrderHash` and `cancel` hashed orders through solc's calldata
  accessors, which revert with no data on an enum out of range or an
  address with dirty upper bits. The optimized decoder copies the words as
  they are (head offsets and lengths masked to four bytes, words past the
  end of calldata read as zero) and hashes them raw. The rewrite now reads
  calldata the same way (`lib/ReferenceCalldataReader.sol`,
  `_deriveOrderComponentsHashFromCalldata`); `cancel` still reads the
  offerer, zone, order type and counter through solc's accessors, as the
  optimized code does. Found by `forge equivalence` with an offer item of
  type 6; test `test_D2_getOrderHashOutOfRangeItemType`.
- D3 `cancel` reverted with `CannotCancelOrder` at the first order the
  caller could not cancel. The optimized code goes through every order and
  reverts after the loop, so a later order that solc's accessors reject
  (an order type out of range) reverts with no data instead. Found by
  `testFuzz_cancel` against the reference; test
  `test_D3_cancelRevertsAfterLoop`.

## Open

- `validate` and every fulfillment and match function: not compared yet.

## Intentional differences

- Gas.
- The TLOAD probe deployed by the constructor is a Solidity contract
  instead of raw init code. Its address and how Seaport calls it
  (`staticcall` with no data and a tenth of the remaining gas) are the
  same; its code is not.
- In transient storage the guard is a `transient` variable at transient
  slot 0 instead of `0x929eee14`. Only the contract itself can read its
  transient storage, so the slot number is not observable.
- The rewrite needs solc 0.8.28 or newer for `transient`.

## Tool limits found

- `forge equivalence` ends incomplete on `getOrderHash` ("solver model
  does not satisfy path constraints involving symbolic Keccak heuristic")
  and on `cancel` (symbolic depth limit), also with `--max-calldata`
  down to 420 bytes, so both are checked by differential fuzzing only.
- `forge equivalence` stops with an incomplete result on symbolic
  BLOCKHASH, so `incrementCounter` is checked by fuzzing only.
- When a counterexample depends on GAS (the probe call forwards
  `gasleft() / 10`), it is reported as incomplete because it did not
  replay, not as a difference. Two mutants of `__activateTstore`
  (wrong error, different probe call data) end that way.
- Whether `forge equivalence` compares the data of calls into unknown
  code is not settled by the above; the differential tests compare calls,
  their data, value and order.
