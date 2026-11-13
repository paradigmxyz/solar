# Seaport 1.6 specs

Function and contract specs for [Seaport 1.6](https://github.com/ProjectOpenSea/seaport)
(commit 1170198) in solar-verify's spec language, kept outside Seaport's sources:

- `seaport.spec`: the specs, as `@custom:` tags grouped under the contract or function they
  belong to. The file's header describes the format.
- `apply_specs.py`: copies a Seaport checkout and inserts each group as NatSpec in front of its
  target. Nothing else in the copy changes, and a target that matches no definition or more than
  one is an error. `--search DIR` points it at another tree that keeps Seaport's contract and
  storage names, such as a high-level rewrite.

  `--define NAME=BODY` overrides a shorthand of the spec file and `--rename FROM=TO` resolves a
  contract under another name, so a rewrite that stores the reentrancy guard in named variables
  can use the same specs:

  ```sh
  apply_specs.py --seaport . --out OUT --search crates/verify/examples/seaport-highlevel/src \
    --rename Consideration=ReferenceConsideration \
    --define SSTORE_GUARD=_reentrancyGuardSstore --define TSTORE_GUARD=_reentrancyGuardTstore
  ```

No spec is a loop invariant: solar-verify has to infer those.

## Running

```sh
git clone https://github.com/ProjectOpenSea/seaport && git -C seaport checkout 1170198
git -C seaport submodule update --init --recursive
python3 crates/verify/seaport/apply_specs.py --seaport seaport --out seaport-spec
cd seaport-spec
solar-verify $(sed -n '/remappings = \[/,/\]/p' foundry.toml | grep -o "'[^']*'" | tr -d "'") \
  --json --jobs 2 --timeout 20000 contracts/Seaport.sol
```

## What the specs say

| Area | Spec |
|---|---|
| Order status (contract) | for every order hash, `numerator <= denominator`, and a cancelled order is not validated |
| Getters | `getOrderStatus`, `getCounter` and `getContractOffererNonce` return the stored values |
| `incrementCounter` | only the caller's counter changes, by less than 2^128 (mod 2^256), and the new value is returned; order status and contract nonces are unchanged |
| `cancel` | every order is the caller's as offerer or zone and is not a contract order; each status it changes ends cancelled and not validated; fills, counters and nonces are unchanged |
| `validate` | only sets `isValidated`: cancellation, fills, counters and nonces are unchanged |
| Fulfill and match (8 functions) | cancellation is unchanged and a cancelled order's fill is unchanged; validation is kept; the filled fraction never decreases; counters are unchanged; the reentrancy guard is clear on return; no native balance is retained (the basic order functions end with the balance they started with) |
| Reentrancy guard (contract) | the guard is the transient slot, or the storage sentinel when transient storage is not in use, at slot `0x929eee14` |

Not yet expressible, so not in the file: that each consideration item is paid to its recipient
(Seaport moves tokens with raw calls from assembly, which no spec can name yet), that a validated
order carries its offerer's signature, and that the order hashes `cancel` touches are the hashes
of the orders it was given.

## Status

On `alpharush/verify-fnptr-slots` (function pointers, free functions and `.slot` in
solar-verify): 8 of 32 functions verify. On `alpharush/verify-tv` alone, 7 do: everything below
except `incrementCounter`.

| Function | Result | Reason |
|---|---|---|
| `getOrderStatus`, `getCounter`, `getContractOffererNonce` | verified | wrong variants of each spec fail with a counterexample |
| `incrementCounter` | verified | the write through `_counters.slot` is `_counters[msg.sender]`; wrong variants of the step bound, the frame and the result fail with a counterexample |
| `__activateTstore`, `_prepareTloadTest` | verified | they keep the order-status invariants |
| `_deriveTypehashes`, `_cast` | verified | no obligations |
| `receive` | fails | it is meant to run only while the guard is held, and `@custom:reentrancy-guard` requires every entry point that reads the guard to revert under it |
| `fulfillOrder`, `fulfillAdvancedOrder`, `fulfillAvailableOrders`, `fulfillAvailableAdvancedOrders`, `matchOrders`, `matchAdvancedOrders`, `validate`, `getOrderHash` | unsupported | a decoder cast to return a struct returns a memory pointer, a reinterpretation the model does not represent |
| `cancel` | unsupported | assembly reads a memory struct with a non-value field (`offer`) |
| `fulfillBasicOrder`, `fulfillBasicOrder_efficient_6GL6yc` | unsupported | assembly may assign `route` a word outside `BasicOrderRouteType` (the range is checked later) |
| `_decode*` (6) | unsupported | memory at symbolic offsets |
| `_decodeOrderComponentsAsOrderParameters` | unsupported | copy into memory at a symbolic location |
| `_decodeGenerateOrderReturndata`, `_performERC1155BatchTransfers` | unsupported | Yul `for` loops |
| `constructor`, `information` | unsupported | assembly writes memory at `0x80` |
| `name` | unsupported | Yul `return` |

These are solver proofs; none is kernel-checked in Lean yet.
