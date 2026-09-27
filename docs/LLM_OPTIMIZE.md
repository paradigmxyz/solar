# LLM-backed MIR optimization

The `llm-optimize` MIR pass asks a rewriter, usually a language model, for cheaper versions of
small functions, and keeps only the candidates it can check. It is off unless
`-Zllm-optimize` is given, so no default build depends on a model, a network, or a cache.

The pass runs on lowered MIR, in the lowered pipeline after `dce` and before stack scheduling,
in gas and size builds. Rewrites are tested against the original, not proved equivalent:
treat them like the output of an unverified optimizer, and benchmark them.

## Modes

`-Zllm-optimize=MODE` selects where rewrites come from:

| Mode     | Rewrites from                                   | Cache (`-Zllm-cache=DIR`)          |
| -------- | ----------------------------------------------- | ---------------------------------- |
| `replay` | The cache only                                  | Required; read, never written      |
| `live`   | The installed rewriter, usually a model         | Optional; read first, then written |
| `script` | Scripted candidates in `-Zllm-script=FILE`      | Optional; read first, then written |

Other flags:

- `-Zllm-rounds=N` (default 6): candidates asked for per function.
- `-Zllm-samples=N` (default 512): generated inputs each candidate runs on.
- `-Zllm-model=MODEL`: the model `live` asks.
- `-Zllm-trace`: print every offer, verdict, and decision on stdout. It also compiles contracts
  one at a time, so the transcript is deterministic.

`live` asks the rewriter an embedder installs (see [Embedding](#embedding)), and sends it the
MIR of every offered function. A typical workflow asks once with a cache and commits what was
found; later builds replay it without a rewriter or a network, and apply each recorded rewrite
only after checking it again:

```bash
solar -Zllm-optimize=replay -Zllm-cache=llm-cache src/Token.sol
```

## What is offered

A function is offered when the interpreter can run everything a call to it reaches, and its
interface is a few words the tests can generate:

- an internal function reachable from an entry, not the dispatch entry, an external entry, an ABI
  wrapper, or a function-pointer dispatcher;
- not recursive, with explicit `iN` or `memptr` parameters and at most one returned word,
  since the backend passes further results through a memory buffer of its own;
- at most 256 instructions;
- only word operations and casts, `select`, phis, `mload`, `mstore`, `mstore8`, `mcopy`,
  `keccak256`, calls to functions that meet the same conditions, and the terminators other than
  `selfdestruct` and `revert_returndata`, with no `undef` values;
- in a module that never reads `msize`, since a candidate may touch memory its original does not.

A function whose generated inputs rarely finish, or leave a reachable block unexecuted, is not
offered either: its candidates could not be tested. `-Zllm-trace` prints the reason for every
function that is not offered.

## Candidate text

The rewriter sees the function as candidate text: its lowered MIR without `!metadata`, printed,
parsed back, and printed again, so value and block numbers follow the text. Candidates use the
same syntax, with a stricter grammar than MIR files:

- exactly one function, with the original's name, parameters, and return type;
- no `!metadata`, because the backend trusts annotations such as memory regions;
- no `undef`, error values, implicit arguments, numeric `fnN` references, `entry`, or ABI layouts;
- calls by `@name`, only to functions the original calls.

## Checks

Each candidate goes through these stages, and the verdict names the one that rejected it:

1. **Parse.** The candidate grammar above, with diagnostics kept out of the compilation.
2. **Constraints.** The signature, operations the interpreter runs and the target EVM version
   has, callees, and `switch` cases that are distinct constants. After dead code elimination,
   the candidate may not keep more values live at once than the original, a bound on the stack
   pressure the cost model cannot see.
3. **Validation.** The MIR validator checks the candidate's body in place of the original's.
4. **Equivalence.** The interpreter runs the original and the candidate on the same inputs. The
   candidate must end the same way (return the same words, revert or return the same data,
   stop, or reach `invalid`), write only memory bytes the original writes, and, when the
   original returns, leave those bytes the same. Every reachable block of the candidate must run
   on some input.
5. **Cost.** The target cost model prices the candidate, which must beat the best so far: by at
   least one stack copy of lifetime gas in gas builds, and in bytes, then gas, in size builds.

The adopted function keeps the original's name, signature, attributes, spans, and memory layout.
Its metadata is empty, and its debug information is marked as intentionally dropped.

### Inputs

Inputs come from a seed derived from the candidate text, so a rerun makes the same decisions.
Arguments mix width boundaries, constants from the original and their neighbors, small numbers,
addresses near the free memory pointer, repeated arguments to exercise aliasing, and random
words, masked to their types. A candidate that adds a constant is also run with that constant in
each argument. Memory holds seeded garbage, except for the zero word at `0x60`, a free memory
pointer from `0x80` up, which is sometimes unaligned, and small words at pointer arguments, which
keep loops over memory short.

Memory writes must stay within the original's because the backend keeps call frames and spill
slots in memory no function addresses, and callers may keep scratch words across a call. A
candidate that uses scratch space its original does not is rejected, even when it restores it.

### Cost model

A function costs the bytes of its reachable code and the average gas of the calls on inputs its
original returns on, including callees and memory growth. Each operation is priced by the target
cost model, with dynamic work sized from the values the run computed. Each operand costs a push
for an immediate or one stack copy otherwise, which stands in for stack scheduling. Accepted
rewrites can still lose to the stack scheduler, so compare them with the runtime benchmarks.

## Sessions

For each offered function the pass opens a session with the rewriter and sends:

- the function's candidate text and the candidate text of its callees;
- the objective, optimizer runs, and EVM version;
- the function's modeled cost;
- the operations a candidate may use.

Each round, the rewriter proposes a candidate and receives the verdict on it: accepted with its
cost, or rejected with the stage, the reason, and for behavior differences, the input that shows
it. The next proposal must beat the best so far. A session ends when the rewriter has nothing
cheaper, after `-Zllm-rounds` rounds, after three rejections in a row, or when the rewriter fails.
A failure never fails the compilation.

Every function is decided against the unchanged module, and rewrites are applied afterwards in
function order, so the result does not depend on parallel compilation.

## Cache

`-Zllm-cache=DIR` holds one file per accepted rewrite, named by a hash of the format version, the
EVM version, the objective, the optimizer runs, and the original's candidate text:

```text
solar llm-optimize cache v1
--- original
fn @sumBelow(arg0: i256) -> i256 [pure] {
  ...
}
--- rewrite
fn @sumBelow(arg0: i256) -> i256 [pure] {
  ...
}
--- evidence
source: live gpt-5
samples: 512
```

An entry applies only when its original matches the function exactly, and its rewrite goes
through every check again, so an edited or stale cache cannot change code the checks reject.
Entries are written to a temporary file and renamed into place.

## Scripts

`-Zllm-optimize=script` replays candidates from a file, for testing the pass. After an optional
preamble, each function's candidates follow its name, and are proposed in order:

```text
--- function sumBelow
--- candidate
fn @sumBelow(arg0: i256) -> i256 [pure] {
  bb0:
    v0 = sub arg0, 1
    v1 = mul arg0, v0
    v2 = shr 1, v1
    ret v2
}
```

The fixtures under `tests/ui/codegen/mir/llm-optimize/` use scripts to cover every verdict.

## Embedding

`solar::codegen::llm` exposes the rewriter interface. An embedder implements `LlmRewriter`, which
opens an `LlmSession` per function, and installs it with `set_rewriter` before compiling with
`-Zllm-optimize=live`. `LlmSession::propose` receives the verdict on the previous candidate and
returns the next candidate or `Proposal::Done`.

## Limits

- Testing is not proof. A candidate that differs only on inputs no generator reaches is
  accepted, such as a comparison with a constant the candidate computes rather than states.
  Proving loop-free candidates with the SMT checker in `scripts/evm-rules/` is future work.
- The interpreter models no storage, calldata, environment, or external calls, so functions that
  use them are not offered.
- The cost model sees one stack copy per operand, not the stack scheduler's decisions.
- `live` sends function MIR to the rewriter's provider.
