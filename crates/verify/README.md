# solar-verify

`solar-verify`, a deductive verifier for Solidity, with mutation testing of its specs and sampled
tests of the specs it trusts.

Specs are written next to the code in NatSpec `@custom:` tags. Every function is proved
**modularly** with Z3: a ✔ means the property holds for all inputs and storage states, under the
trusted base listed below. Nothing in a proof is bounded, sampled or fuzzed; `--spec-check` (see
Testing assumptions) samples the trusted base itself.

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
- `--instrument OUT.sol`: write the file with every `@custom:check` as a `require`. A check in an
  imported file is an error, since the verification assumed it.
- `--spec-check`: test the trusted base against its code in the file instead of verifying (see
  Testing assumptions); `--spec-check-runs N` samples per function (default 256), `--seed N`.

Imports resolve as with `solar` and solc, so a project needs no flattening: pass remappings as
`PREFIX=PATH` (or `CONTEXT:PREFIX=PATH`), and `--base-path PATH` and `--include-path PATH` (`-I`).
In a Foundry project, `solar-verify $(cat remappings.txt) src/Vault.sol` reads the project's
remappings. Names resolve as if the files were flattened, so an import that renames or
namespaces (`{A as B}`, `* as M`, `"a.sol" as M`) and a name two files define are errors.
Contracts from imported files are not verified, but the functions a verified contract inherits
from them are, as its own. `mutate` takes the same options, and verifies each mutant, and compiles
it with solc for TCE, at the file's own path.

Without Z3 the tests that need it are skipped with a message on stderr; set `SOLAR_VERIFY_REQUIRE_Z3=1`
to make them fail instead.

## Spec language

Specs live in NatSpec tags, so annotated contracts compile unchanged with solc and cost no gas.
NatSpec tag names are lowercase letters and `-`.

| Where | Tag | Meaning |
|---|---|---|
| function | `@custom:requires e` | precondition: assumed in the body, checked at every internal call site |
| function | `@custom:ensures e` | postcondition: checked on every normal return, assumed by callers |
| function | `@custom:succeeds-if e` | from a state where `e` and the requires hold, the function returns normally. A call to a function with a contract may revert unless that function's own `succeeds-if` holds at the call; an external call may revert unless a `try` catches it or its interface function has a `succeeds-if` that holds (trusted) |
| function | `@custom:trusted` | the body is not verified; callers use its contract (`--spec-check` tests it) |
| function | `@custom:spec` | a single-`return` function usable in specs (pure and view ones are by default) |
| function | `@custom:opaque` | a spec function seen as uninterpreted, except through `@custom:reveal` |
| function | `@custom:broadcast` | a lemma whose requires ==> ensures, for all values of its parameters, is an axiom where `@custom:broadcast-use` names it (see Broadcast lemmas) |
| function | `@custom:trigger e1, e2` | on a broadcast lemma: terms whose instances make the solver apply it, as one multi-pattern; each tag adds a pattern |
| function | `@custom:broadcast-use L G` | this function's proof assumes broadcast lemmas `L` and the lemmas of group `G` (`C.L` names another contract's) |
| function | `@custom:check P` | a claim enforced at runtime by `--instrument` and then assumed; callers are never blamed for it |
| contract | `@custom:invariant e` | assumed on entry to public and external functions, checked at each exit, after the constructor, and before every external call |
| contract | `@custom:group G L1 G2` | a named group of broadcast lemmas and groups; groups nest, and a group that contains itself is an error |
| contract | `@custom:broadcast-use L G` | as on a function, for every function of the contract except broadcast lemmas |
| contract | `@custom:ghost T name` | ghost state variable, zero at deployment |
| contract | `@custom:reentrancy-guard v` | `v` (a variable or a boolean expression) is a lock: entry points that use it must revert while it holds |
| contract | `@custom:lock-stable P` | every entry point callable under the lock keeps `P` |
| struct | `@custom:invariant P` | holds of every value of the struct stored, constructed or read from storage |
| interface | `@custom:requires` / `ensures` | the contract of an external call: callers prove the requires, the callee's ensures are trusted |
| statement | `@custom:invariant e` | loop invariant (optional: invariants are inferred, see Loops) |
| statement | `@custom:decreases e` | termination measure: stays `>= 0` and strictly decreases each iteration (optional: one is inferred from the guard) |
| statement | `@custom:assert e` | proved here, then assumed |
| statement | `@custom:assume e` | assumed without proof |
| statement | `@custom:ghost x = e` | ghost update, run before the statement |
| statement | `@custom:check P` | as on a function, before the statement |
| statement | `@custom:use L(args)` | proves lemma `L`'s requires here and assumes its ensures; `L` is a pure function verified on its own |
| statement | `@custom:reveal f(args)` | unfolds opaque `f` once at that application |
| statement | `@custom:broadcast-use L G` | as on a function, from this statement on along its path |

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

## Ghost code

Ghost updates, spec expressions, the lemmas `@custom:use` applies and `@custom:broadcast` lemmas run
only in the proof, so a proof holds of the deployed contract only if erasing them changes no real
execution. Before any solver runs, solar-verify checks that ghost and real code do not interfere,
after Filliâtre, Gondelman and Paskevich,
[The Spirit of Ghost Code](https://inria.hal.science/hal-00873187v3) (CAV 2014), and
verifies nothing in a file that breaks a rule:

- ghost code writes only `@custom:ghost` variables; it may read real state;
- ghost code never reverts (`require`, `assert`, `revert`), makes no external call, and no ghost
  condition guards a real write;
- ghost code terminates: it calls only pure and view functions, every loop in a lemma or a function
  a ghost update calls has a `@custom:decreases` variant, and those functions are not recursive;
- real code, including `@custom:check` claims, never reads a ghost variable, and no real declaration
  takes a ghost's name.

A lemma must also return normally wherever its requires hold, since its ensures are assumed at each
use: that is one more obligation of the lemma and of the functions it runs. `@custom:broadcast-use`,
`@custom:group` and `@custom:trigger` are spec-only: they name lemmas or are spec expressions.

## Broadcast lemmas

`@custom:use` applies a lemma at one statement. A `@custom:broadcast` lemma instead applies wherever
its trigger matches, after Verus's [broadcast proofs](https://verus-lang.github.io/verus/guide/broadcast_proof.html)
and [Tunable Automation in Automated Program Verification](https://arxiv.org/abs/2512.03926):

```solidity
library L {
    /// @custom:opaque
    function f(uint256 x) internal pure returns (uint256) { return x + 1; }

    /// @custom:broadcast
    /// @custom:ensures f(x) > x
    function f_grows(uint256 x) internal pure {
        /// @custom:reveal f(x)
        return;
    }
}

/// @custom:broadcast-use L.f_grows
contract C {
    /// @custom:ensures L.f(L.f(a)) > a + 1
    function twice(uint256 a) public pure {}
}
```

- A broadcast lemma is pure, returns nothing and takes value-type parameters. It is verified on its
  own like any function; its axiom, `forall` parameters in their ranges, requires `==>` ensures, is
  assumed only where `@custom:broadcast-use` names it or a group containing it, and only once its
  proof succeeded. A lemma that fails, is `@custom:trusted` or has no body is never assumed, and its
  users are told so.
- The solver instantiates the axiom at the terms its triggers match: the `@custom:trigger` terms,
  else the innermost applications of uninterpreted functions (opaque spec functions, and pure
  functions with more than a `return`) in its ensures that mention every parameter. A trigger must
  apply such a function; one made of arithmetic or comparisons only is rejected, as is a lemma with
  no trigger. Spec functions with a single `return` are inlined, so make them `@custom:opaque` to
  trigger on them.
- No proof assumes a lemma that rests on it: a function's proof leaves out every broadcast lemma
  whose proof reaches the function through calls, `@custom:use` or broadcasts, the lemma itself
  included. A broadcast lemma's own proof takes only the broadcasts on the lemma and its statements,
  never the contract's.

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

## Loops

A loop is cut at its head with its invariants: they must hold on entry and after every
iteration, and are all the loop's exit knows. Writing them is optional. Inference (`src/vc/infer.rs`)
mines candidates at the head: relations between what the loop writes and what it reads (variables,
array lengths, values at loop entry, constants in the loop), the function's specs and contract
invariants, also with a bound replaced by a counter the loop advances (`count == old(count) + n`
becomes `count == old(count) + i`), and facts from the abstract interpreter. Houdini keeps the
largest subset that holds on entry and is preserved by the body, running the body in a solver
scope it then pops. The survivors are then checked like hand-written invariants, so no solver
answer given during inference is trusted, and a query that times out only drops a candidate. A
termination measure is picked among the distances the guard's comparisons bound and its
obligations are checked the same way; a loop without one is verified for partial correctness only.

- `--show-inferred`: print what each loop kept, where it came from, and its measure.
- `--no-infer`: use the hand-written invariants only.

## Limits

Supported: value types, enums, structs, nested mappings, storage and memory dynamic arrays,
modifiers, inheritance (across imported files too), libraries and `using for`, internal, interface and
low-level calls, try/catch, tuples, `unchecked`, casts, units, `keccak256`, `abi.encode` and typed
`abi.decode`, ETH balances of this contract, and transient storage.

Inline assembly runs in the same executor with EVM word semantics, `sload`/`sstore` over raw
storage, and memory at constant offsets. `sload(p.slot)` and `sstore(p.slot, v)` work on a local
storage pointer `p` to a struct whose fields pack into one slot (`examples/struct_slot.sol`).

Not supported: Yul `for` loops, memory at symbolic offsets, `.slot` and `.offset` of anything else,
`do`/`while` loops, imports that rename or namespace, storage pointers chosen by conditionals, function
pointers, the code of other accounts, and triggers on spec quantifiers (`@custom:trigger` applies to
broadcast lemmas). Unsupported constructs are reported as errors, never skipped, so a ✔ is never
produced over code solar-verify did not model.

## Trusted base

- The parser, the encoding in `src/vc.rs` and `src/vc/yul.rs`, and the solvers.
- `@custom:assume`, `@custom:trusted`, and interface ensures. Broadcast lemmas are axioms only once
  proved, and `src/broadcast.rs` keeps every proof from resting on a lemma that rests on it. Ghost
  updates are checked to be erasable (see Ghost code) but their values are whatever they compute.
  `--spec-check` tests trusted functions and interface specs whose code is in the file, on samples
  (see Testing assumptions).
- Other contracts reach this contract's storage only through its public functions: no
  `delegatecall` into it.
- `keccak256` is injective and its results lie in `[2^128, 2^256)`, so they never collide with
  small constant slots.
- Raw storage slots that assembly reaches are disjoint from the declared state variables, and
  assembly is memory-safe.
- A library named `Math` with a `mulDiv(x, y, denominator)` and no specs of its own is summarized
  as OpenZeppelin's `Math.mulDiv`; `--spec-check` tests the summary against the library's code.
- A high-level call to a `view` or `pure` function answers as a function of its target, its
  arguments and the state it can read (this contract's storage, ETH balances and the code at every
  address), so two such calls with nothing in between answer alike. A callee whose answer depends
  on the gas it is given breaks this.

## Testing assumptions

`solar-verify --spec-check FILE.sol` tests the contracts the proofs trust but do not prove, after
Verus's [SpecCheck](https://github.com/verus-lang/verus-spec-check), against the code the file
contains:

- each `@custom:trusted` function with a body and specs, in its own contract when that deploys
  (a library's internal functions get a wrapper contract), else in the first contract in the
  file that inherits it and deploys;
- each interface function with specs, in every contract of the file that inherits the interface
  and implements it;
- the built-in `Math.mulDiv` summary, against the `Math.mulDiv` of the file.

The file is compiled with Solar, with a harness added: an external wrapper for each internal or
private function under test, and a setter and a getter for each state variable of a value type
and each mapping from a value type to a value type. Each sample deploys the contract in a fresh
in-process EVM (Osaka, zero constructor arguments), writes random values to that state (mapping
entries at the sample's arguments, the caller, the contract and zero), and calls the function
with arguments drawn from boundary values (0, 1, the type's extremes, powers of two and their
neighbours) and random ones. The specs are evaluated at the observed values with the verifier's
own encoding and Z3, so they mean what the proofs assume:

- where the requires hold (and, for a public or external function, the contract invariants) and
  the call returns, the ensures (and the invariants) must hold;
- where the requires and the succeeds-if hold, the call must not revert.

```text
$ solar-verify --spec-check tests/cases/speccheck.sol
✘ Vault.bad_inc (@custom:trusted): 1 of its specs violated; 256 samples, requires held in 123, returned in 123
error: ensures does not hold
   ╰ note: counterexample: x = 7; returned result = 7
...
✔ Vault.ok_vacuous (@custom:trusted): 256 samples, requires held in 0, returned in 0
warning: Vault.ok_vacuous (@custom:trusted): no sample satisfied the requires: the check is vacuous
```

Each violated spec is reported once, with the first counterexample: the arguments, the state the
specs read, and what the call returned or that it reverted; the exit status is then 1. The counts
show how often the requires held and the call returned, and a warning marks a check no sample
reached. A sample whose specs read what the harness does not observe (ghost state, mapping
entries at other keys, structs, arrays and nested mappings, which keep their constructor values)
counts as undetermined. A function whose parameters or returns are not value types, a public
library function, or a contract whose constructor reverts on zero arguments is listed as not
tested. This is testing, not proof: a check that holds may fail on inputs no sample drew.

## Spec mutation testing

`solar-verify mutate FILE.sol` measures how much a file's specs pin down. It mutates the bodies of
functions that verify and that some spec reaches, re-verifies each mutant in the mutated function
and every function that can reach it, and reports each surviving mutant as a warning at the
mutated code. Only a failure with a counterexample kills a mutant.

The operators replace relational (ROR), arithmetic (AOR) and logical (LCR) operators, booleans
(BOOL) and decimal constants (CRP), and delete expression statements (SDL) and `if` guards that
only revert (GDL). Inline assembly and the bodies of `@custom:trusted` functions, which are not
verified, are not mutated.

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
