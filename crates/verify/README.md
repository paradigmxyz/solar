# solar-verify

`solar-verify`, a deductive verifier for Solidity, with mutation testing of its specs.

Specs are written next to the code in NatSpec `@custom:` tags. Every function is proved
**modularly** with Z3: a ✔ means the property holds for all inputs and storage states, under the
trusted base listed below. Nothing is bounded, sampled or fuzzed.

```text
$ solar-verify examples/Bugs.sol
✘ Bugs.payout (1 of 2 obligations failed)
error: contract invariant must hold before an external call (reentrancy)
   ╭▸ examples/Bugs.sol:36:9
   │
36 │         r.onPayout(amount);
   │         ━━━━━━━━━━━━━━━━━━
   │
   ├ note: in Bugs.payout
   ├ note: failing spec: reserved <= deposits
   ├ note: spec declared at line 9; checked at line 36
   ╰ note: counterexample (pre-state): msg.sender = 1, deposits = 1, reserved = 1, r = 0, amount = 1
```

## Running

Install Z3 (any version from 4.8) on `PATH`, or point `SOLAR_VERIFY_Z3` at it. When cvc5 and Bitwuzla
are on `PATH` (or named by `SOLAR_VERIFY_CVC5` and `SOLAR_VERIFY_BITWUZLA`; `off` disables either), they
race Z3 on the queries it gives up on.

```sh
cargo build -p solar-verify --release
target/release/solar-verify crates/verify/examples/Token.sol
cargo nextest run -p solar-verify
```

Flags:

- `--strict`: overflow, division by zero, out-of-bounds access and `pop` on an empty array are
  errors instead of reverting paths.
- `--timeout MS`: per obligation, default 10000.
- `--only Contract[.fn]`: verify one contract or function.
- `--jobs N`: verify functions in N processes.
- `--json`: also print one JSON row per function.
- `--dump-smt FILE`: write the SMT-LIB sent to the solver.
- `--dry-run`: generate every obligation without running a solver (with `--dump-smt`).
- `--instrument OUT.sol`: write the file with every `@custom:check` as a `require`.

Without Z3 the tests that need it are skipped with a message on stderr; set `SOLAR_VERIFY_REQUIRE_Z3=1`
to make them fail instead.

## Spec language

Specs live in NatSpec tags, so annotated contracts compile unchanged with solc and cost no gas.
NatSpec tag names are lowercase letters and `-`.

| Where | Tag | Meaning |
|---|---|---|
| function | `@custom:requires e` | precondition: assumed in the body, checked at every internal call site |
| function | `@custom:ensures e` | postcondition: checked on every normal return, assumed by callers |
| function | `@custom:succeeds-if e` | from a state where `e` and the requires hold, the function returns normally |
| function | `@custom:trusted` | the body is not verified; callers use its contract |
| function | `@custom:spec` | a single-`return` function usable in specs (pure and view ones are by default) |
| function | `@custom:opaque` | a spec function seen as uninterpreted, except through `@custom:reveal` |
| function | `@custom:check P` | a claim enforced at runtime by `--instrument` and then assumed; callers are never blamed for it |
| contract | `@custom:invariant e` | assumed on entry to public and external functions, checked at each exit, after the constructor, and before every external call |
| contract | `@custom:ghost T name` | ghost state variable, zero at deployment |
| contract | `@custom:reentrancy-guard v` | `v` (a variable or a boolean expression) is a lock: entry points that use it must revert while it holds |
| contract | `@custom:lock-stable P` | every entry point callable under the lock keeps `P` |
| struct | `@custom:invariant P` | holds of every value of the struct stored, constructed or read from storage |
| interface | `@custom:requires` / `ensures` | the contract of an external call: callers prove the requires, the callee's ensures are trusted |
| statement | `@custom:invariant e` | loop invariant |
| statement | `@custom:decreases e` | termination measure: stays `>= 0` and strictly decreases each iteration |
| statement | `@custom:assert e` | proved here, then assumed |
| statement | `@custom:assume e` | assumed without proof |
| statement | `@custom:ghost x = e` | ghost update, run before the statement |
| statement | `@custom:check P` | as on a function, before the statement |
| statement | `@custom:use L(args)` | proves lemma `L`'s requires here and assumes its ensures; `L` is a pure function verified on its own |
| statement | `@custom:reveal f(args)` | unfolds opaque `f` once at that application |

A statement spec attaches to the statement after it. An `@custom:assert` or `@custom:assume` after
the last statement of a block runs when the block ends; any other spec that no statement carries
is an error.

Spec expressions are Solidity expressions over **mathematical integers** (no overflow in specs),
plus:

- `old(e)`: the value of `e` in the function's pre-state.
- `result`: the single return value (named return variables work too).
- `a ==> b` and `a <==> b`.
- `forall uint256 i, address a :: body` and `exists T x :: body`, with binders ranging over their
  types.
- `__time`: the latest `block.timestamp` the contract has seen.
- Calls to spec functions.

## Semantics

`require`, `revert` and Solidity's checked-arithmetic panics end their path: a reverted
transaction has no effect, so postconditions and invariants are owed only on normal returns.
`--strict` makes panics proof obligations. Solidity `assert` is always one.

A call to a function with a contract is replaced by that contract: check its requires, havoc what
it may write, assume its ensures. Functions without a contract are inlined, so recursion needs a
contract. Loops are cut at their invariants.

An external call may reenter any public function, so the contract invariants must hold before it,
and storage is havocked across it. Under a `@custom:reentrancy-guard` lock, only what the entry
points that stay callable under the lock can write is havocked. View and pure calls and
`.transfer` havoc nothing.

If no execution of a function can return normally, solar-verify warns, so contradictory specs cannot
verify everything silently.

## Limits

Supported: value types, enums, structs, nested mappings, storage and memory dynamic arrays,
modifiers, inheritance within one file, libraries and `using for`, internal, interface and
low-level calls, try/catch, tuples, `unchecked`, casts, units, `keccak256`, `abi.encode` and typed
`abi.decode`, ETH balances of this contract, and transient storage.

Inline assembly runs in the same executor with EVM word semantics, `sload`/`sstore` over raw
storage, and memory at constant offsets. `sload(p.slot)` and `sstore(p.slot, v)` work on a local
storage pointer `p` to a struct whose fields pack into one slot (`examples/struct_slot.sol`).

Not supported: Yul `for` loops, memory at symbolic offsets, `.slot` and `.offset` of anything else,
`do`/`while` loops, imports, storage pointers chosen by conditionals, function pointers, the code of
other accounts, and quantifier triggers. Unsupported constructs are reported as errors, never
skipped, so a ✔ is never produced over code solar-verify did not model.

## Trusted base

- The parser, the encoding in `src/vc.rs` and `src/vc/yul.rs`, and the solvers.
- Ghost updates, `@custom:assume`, `@custom:trusted`, and interface ensures.
- Other contracts reach this contract's storage only through its public functions: no
  `delegatecall` into it.
- `keccak256` is injective and its results lie in `[2^128, 2^256)`, so they never collide with
  small constant slots.
- Raw storage slots that assembly reaches are disjoint from the declared state variables, and
  assembly is memory-safe.
- A library named `Math` with a `mulDiv(x, y, denominator)` and no specs of its own is summarized
  as OpenZeppelin's `Math.mulDiv`.

## Spec mutation testing

`solar-verify mutate FILE.sol` measures how much a file's specs pin down. It mutates the bodies of
functions that verify and that some spec reaches, re-verifies each mutant in the mutated function
and every function that can reach it, and reports each surviving mutant as a warning at the
mutated code. Only a failure with a counterexample kills a mutant.

The operators replace relational (ROR), arithmetic (AOR) and logical (LCR) operators, booleans
(BOOL) and decimal constants (CRP), and delete expression statements (SDL) and `if` guards that
only revert (GDL). Inline assembly is not mutated.

```text
$ solar-verify mutate --per-fn 3 examples/Counter.sol
killed     Counter.constructor SDL line 11: `limit = _limit;` -> `` (by Counter.constructor)
...
survived   Counter.max ROR line 34: `>=` -> `>`
mutation results:: 7 killed, 1 survived, 0 equivalent, 0 undecided, 0 stillborn; score 7/8 = 88%
```

Flags:

- `--list`: print the mutants of every function a spec reaches, without running the verifier.
- `--json`: also print one JSON row per mutant.
- `--ops ROR,AOR,...`: the operators to use (default all).
- `--per-fn N`: at most N mutants per function (default 12), drawn round-robin over its operators
  in a `--seed`ed order.
- `--max-affected N`: re-verify at most N functions per mutant (default 8).
- `--only Contract[.fn]`, `--jobs N`, `--timeout MS`, `--strict`: as for verification.
- `--solc PATH`, `--no-tce`: with solc (from `--solc`, `$SOLC` or `PATH`), a survivor whose
  optimized bytecode equals the original's is equivalent and leaves the score; one in a function
  no bytecode contains, or in an internal function or modifier, stays a survivor with a note.
