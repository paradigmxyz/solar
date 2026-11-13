# Differences from the optimized Seaport 1.6

## Found and fixed in the rewrite

Each was a difference between Seaport's `reference/` and the optimized
contracts. Fixed differences found by fuzzing have a test in
test/Regressions.t.sol.

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

## Open

- D2 `getOrderHash` and `cancel` take orders whose enum fields (item
  type, order type) are out of range: the optimized code hashes the raw
  byte, the solc ABI decoder in the rewrite reverts with no data. Found by
  `forge equivalence` with an offer item of type 6. The fix is to decode
  the external structs with `uint8` fields (same ABI and selectors) and
  check ranges only where the optimized code does.

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

- `forge equivalence` stops with an incomplete result on symbolic
  BLOCKHASH, so `incrementCounter` is checked by fuzzing only.
- When a counterexample depends on GAS (the probe call forwards
  `gasleft() / 10`), it is reported as incomplete because it did not
  replay, not as a difference. Two mutants of `__activateTstore`
  (wrong error, different probe call data) end that way.
- Whether `forge equivalence` compares the data of calls into unknown
  code is not settled by the above; the differential tests compare calls,
  their data, value and order.
