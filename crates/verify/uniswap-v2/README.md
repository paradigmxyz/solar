# Uniswap V2 specs

**Status: SMT only.** These specs are checked by solar-verify's SMT backend; none is
kernel-checked in Lean, and none is part of the certified pipeline.

Function and contract specs for the core Uniswap V2 contracts (UniswapV2Pair, UniswapV2ERC20,
UniswapV2Factory, Math) in solar-verify's spec language, kept outside the sources:

- `uniswap-v2.spec`: the specs, as `@custom:` tags grouped under the contract, function or
  statement they belong to. Statement targets carry only ghost updates (`@custom:ghost`), which
  record a value for the postconditions and are checked not to interfere with real execution.
- `apply_specs.py`: copies the ^0.8 port that `solar-tv/uniswap-v2/port.py` writes and inserts
  each group as NatSpec in front of its target. Nothing else in the copy changes, and a target that
  matches no definition or more than one (or no line, or more than one, for a statement) is an
  error.
- `spec_mutants.py`: mutates the code with `solar-verify mutate`'s operators, re-verifies what
  each mutant reaches, and reports for every spec the mutants that make it fail with a
  counterexample. Unknown results never count as kills.

No spec is a loop invariant: `Math.sqrt`'s loop invariants are inferred.

## Running

```sh
python3 solar-tv/uniswap-v2/port.py --out target/uniswap-v2/src
python3 crates/verify/uniswap-v2/apply_specs.py --src target/uniswap-v2/src --out target/uniswap-v2/spec
cd target/uniswap-v2/spec
for f in UniswapV2Factory.sol libraries/Math.sol UniswapV2ERC20.sol UniswapV2Pair.sol; do
  solar-verify --json --jobs 4 --timeout 120000 "$f"
done
python3 crates/verify/uniswap-v2/spec_mutants.py --tree target/uniswap-v2/spec \
  --verifier solar-verify --file UniswapV2Pair.sol --file UniswapV2ERC20.sol \
  --also UniswapV2ERC20.sol=UniswapV2Pair.sol:UniswapV2Pair.mint \
  --file UniswapV2Factory.sol --file libraries/Math.sol --per-fn 100 --timeout 8000
```

## What the specs say

| Area | Spec |
|---|---|
| ERC20 supply | `totalSupply` equals a ghost sum of all balances, kept by ghost updates in `_mint` and `_burn` |
| ERC20 transfers | `transfer` and `transferFrom` move `value` between the two balances and no others; `transferFrom` spends the allowance unless it is infinite; `approve` and `permit` set exactly one allowance, and `permit` bumps the owner's nonce |
| Locked liquidity | nobody holds an allowance from the zero address, and the zero address's balance never drops below what was minted to it (ghost `zeroMinted`); the first `mint` adds `MINIMUM_LIQUIDITY` to it |
| `mint` | first mint: liquidity is `floor(sqrt(amount0 * amount1)) - MINIMUM_LIQUIDITY`; later mints: the largest amount with `liquidity * reserve <= amount * supply` for both tokens; the supply grows by exactly what was minted; reserves grow by the deposits |
| `burn` | each amount is `floor(liquidity * balance / supply)`, both positive; the supply shrinks by the burned liquidity |
| `swap` | the fee-adjusted new balances keep `reserve0 * reserve1 * 1000^2`, so `k` never decreases; some input came in; outputs were below the reserves; the supply is unchanged |
| `sync`, `skim`, `_update` | `_update` sets the reserves to the balances (so they fit in uint112) and the timestamp to the block's, mod 2^32; `skim` keeps the reserves |
| Lock | `@custom:reentrancy-guard unlocked != 1`: every lock-guarded function reverts while the lock is held, and each leaves it released |
| Factory | `getPair` is symmetric, never pairs a token with itself, records only pairs it created, and maps each pair address to one unordered pair of tokens; `createPair` records a new nonzero pair in both directions and appends it to `allPairs`; only `feeToSetter` changes the fee settings |
| Math | `sqrt` is the floor square root; `min` is the minimum |

## Status

SMT only. On the port from `port.py`, every function verifies against its specs: Factory 5/5,
Math 2/2, UniswapV2ERC20 7/7 and UniswapV2Pair 13/13. `Math.sqrt`'s loop invariants are
inferred. `UniswapV2Pair.swap` needs
`--timeout 120000`: the port's unchecked `SafeMath.mul` turns its overflow check into a
multiplication followed by a division, which the solver takes minutes over.

Mutation testing has not reached the bar of a failing mutant for every spec: 12 of the 70 specs
are killed. Most mutants of functions whose contract has a quantified invariant (the ERC20's
`forall address s :: allowance[address(0)][s] == 0` and the Factory's `getPair` invariants)
come back unknown ("incomplete quantifiers") instead of with a counterexample, even for a
`transfer` that credits `value + 1`. Without the quantified invariant, that mutant fails with a
counterexample in under a second. Counterexamples under quantified invariants are a solar-verify gap.

These are solver results; none is kernel-checked in Lean yet.
