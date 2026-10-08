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
| `live`   | A model, or a rewriter an embedder installs     | Optional; read first, then written |
| `script` | Scripted candidates in `-Zllm-script=FILE`      | Optional; read first, then written |

Other flags:

- `-Zllm-rounds=N` (default 6): candidates asked for per function.
- `-Zllm-samples=N` (default 512): generated inputs each candidate runs on.
- `-Zllm-model=PROVIDER/MODEL`: the model `live` asks, such as `gpt-6-astra` (the default),
  `anthropic/claude-opus-5-5`, or `opencode/deepseek-v4.1-flash`; see [Live mode](#live-mode).
- `-Zllm-endpoint=URL`: the API base URL `live` asks instead of the provider's, such as a proxy.
- `-Zllm-effort=EFFORT`: how much the model reasons: `none`, `low`, `medium`, `high`, `xhigh`,
  or `max`.
- `-Zllm-trace`: print every offer, verdict, and decision on stdout. It also compiles contracts
  one at a time, so the transcript is deterministic.

A typical workflow asks a model once with a cache and commits what it found; later builds replay
the cache without a model or a network, and apply each recorded rewrite only after checking it
again:

```bash
cargo install --locked --path crates/solar --features llm
OPENAI_API_KEY=... solar -Zllm-optimize=live -Zllm-cache=llm-cache --emit=bin src/Token.sol
solar -Zllm-optimize=replay -Zllm-cache=llm-cache --emit=bin src/Token.sol
```

Other providers' models take the same flags:

```bash
ANTHROPIC_API_KEY=... solar -Zllm-optimize=live -Zllm-model=anthropic/claude-opus-5-5 \
  -Zllm-effort=high -Zllm-cache=llm-cache --emit=bin src/Token.sol
OPENCODE_ZEN_API_KEY=... solar -Zllm-optimize=live -Zllm-model=opencode/deepseek-v4.1-flash \
  -Zllm-cache=llm-cache --emit=bin src/Token.sol
```

## Live mode

The command line supports `live` when the compiler is built with its `llm` feature; without it,
`live` is an error. `-Zllm-model` picks the provider and the model:

| `-Zllm-model`           | API                                                                    | Key                    |
| ----------------------- | ---------------------------------------------------------------------- | ---------------------- |
| `MODEL`, `openai/MODEL` | OpenAI's Responses API, through [nanocodex](https://docs.rs/nanocodex) | `OPENAI_API_KEY`       |
| `anthropic/MODEL`       | Anthropic's Messages API at `https://api.anthropic.com/v1`             | `ANTHROPIC_API_KEY`    |
| `opencode/MODEL`        | OpenCode Zen's chat completions at `https://opencode.ai/zen/v1`        | `OPENCODE_ZEN_API_KEY` |
| `openai-chat/MODEL`     | OpenAI's chat completions at `https://api.openai.com/v1`               | `OPENAI_API_KEY`       |

The key goes only to the provider's client. `-Zllm-endpoint` replaces the base URL, for a proxy or
another server that speaks the same API. The compiler warns that `live` sends the MIR of every
offered function to the provider, or to the host `-Zllm-endpoint` names, and ends with a note of
the turns, tokens, and estimated cost it spent.

Each offered function gets its own conversation, which opens with the rewriting brief in
`crates/cli/src/llm/instructions.md`: the syntax and semantics of lowered MIR, what a candidate
must preserve, the costs, and the reply format. Models get no tools, so they cannot read files,
run commands, or search; OpenAI agents also get a fixed environment, so they see neither the
host's date nor its `AGENTS.md`. Each reply must hold one fenced `mir` block or
`NO_IMPROVEMENT`; a reply with neither gets one reminder. Anthropic and OpenCode Zen take the
whole conversation with every request, and each reply goes back as the provider sent it,
reasoning included: Claude's thinking blocks, and DeepSeek's `reasoning_content`. Anthropic
requests mark the brief and the newest prompt for prompt caching, so a turn rereads the
conversation from the cache.

While `live` works, every conversation reports on stderr: each round, the model's reasoning
(marked `┆`) and reply (marked `│`) as they stream in, how long each turn took and what it used,
each verdict, and what the pass keeps. Lines name the module and function, so conversations that
run at once interleave by whole lines, and a request sent again says why. A function whose
rewrite `-Zllm-cache` already holds says so and is not sent to the model; build without the cache,
or delete the function's entry, to ask again. Nothing is printed when `--error-format` is
machine-readable. A short conversation reads:

```text
llm-optimize Triangle @sumBelow: costs 5047 gas, 47 bytes; asking opencode/deepseek-v4.1-flash for something cheaper
llm-optimize Triangle @sumBelow: round 1
  Triangle @sumBelow ┆ The loop adds 0 through n - 1, an arithmetic series.
  Triangle @sumBelow │ ```mir
  Triangle @sumBelow │ fn @sumBelow(arg0: i256) -> i256 [pure] {
  ...
llm-optimize Triangle @sumBelow: replied in 41.3 s using 5062 tokens, an estimated $0.0049
llm-optimize Triangle @sumBelow: accepted at 81 gas, 32 bytes
llm-optimize Triangle @sumBelow: round 2
  Triangle @sumBelow │ NO_IMPROVEMENT
llm-optimize Triangle @sumBelow: replied in 9.8 s using 1320 tokens, an estimated $0.0006
llm-optimize Triangle @sumBelow: the model has nothing cheaper
llm-optimize Triangle @sumBelow: keeps a rewrite at 81 gas, 32 bytes, down from 5047 gas, 47 bytes
```

`-Zllm-effort` sets how much the model reasons, in its provider's terms: nanocodex's thinking
level for OpenAI, adaptive thinking at that `output_config.effort` for Anthropic (`none` turns
thinking off), and `reasoning_effort` for OpenCode Zen. Without it, the model reasons as its
provider defaults. The compiler knows two models' efforts and prices, and rejects an effort
either model does not take:

| Model                          | Efforts                                 | Price per million tokens                               |
| ------------------------------ | --------------------------------------- | ------------------------------------------------------ |
| `anthropic/claude-opus-5-5`    | `low`, `medium`, `high`, `xhigh`, `max` | $4 input, $20 output, $5 cache write, $0.20 cache read |
| `opencode/deepseek-v4.1-flash` | `low`, `high`, `max`                    | $0.30 input, $1.20 output, $0.006 cache read           |

nanocodex prices OpenAI's models. Any other model is asked all the same, and the note reports its
tokens without a cost.

At most four turns run at once, a turn that takes more than ten minutes fails its session, and no
turn starts once the estimated spend reaches five dollars or the conversations have used ten
million tokens, which bounds a model without known prices. Anthropic and OpenCode Zen requests
that meet a rate limit, an overloaded or failing server, or a failed connection are sent up to
four times, waiting between tries as long as the provider asks, or two seconds and then twice as
long each time. A failed session leaves its function with the best candidate so far.

## What is offered

A function is offered when the interpreter can run everything a call to it reaches, and its
interface is a few words the tests can generate:

- an internal function reachable from an entry, not the dispatch entry, an external entry, an ABI
  wrapper, or a function-pointer dispatcher;
- not recursive, with explicit `iN` or `memptr` parameters and at most one returned word,
  since the backend passes further results through a memory buffer of its own;
- outside the constructor, and calling nothing recursive: the backend keeps the frames of such
  calls on the heap, where a call writes memory above the free memory pointer and, when the
  callee takes or returns memory, leaves the pointer raised past its frame, neither of which the
  tests model;
- at most 256 instructions;
- only word operations and casts, `select`, phis, `mload`, `mstore`, `mstore8`, `mcopy`,
  `keccak256`, persistent and transient storage, logs, reads of the call and block context other
  than code sizes and hashes, calls to functions that meet the same conditions, and the
  terminators other than `selfdestruct` and `revert_returndata`, with no `undef` values;
- in a module that never reads `msize`, since a candidate may touch memory its original does not.

The interpreter runs each instruction by the semantics its operation schema row declares
(`crates/codegen/src/mir/semantics.rs`), computing pure opcodes with the opcode table's word
semantics, the same definitions constant folding uses. Storage, transient storage, logs, and
context reads run against a world the tests derive for each input. Code sizes and hashes stay
unsupported because a rewrite changes the contract's own code, and so do `gas`, calldata, and
calls to other contracts.

A function is not offered either when its generated inputs rarely finish or do not exercise it:
every reachable block must run to its end, every decision must come out both ways, and every
other value must come out nonzero, on some input. A revert discards what its run wrote, so in the
blocks from which the function can end without reverting, only inputs on which it does not revert
count, except for a decision's outcome that branches into blocks that always revert.
`-Zllm-trace` prints the reason for every function that is not offered.

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
   has, callees, tail calls, and `switch` cases that are distinct constants. A candidate tail
   calls only as `lower-evm-shaped` does: to a function that never returns and is not
   recursive, from code the constructor does not run. After dead code elimination, the candidate
   may not keep more values live at once than the original, a bound on the stack pressure the
   cost model cannot see.
3. **Validation.** The MIR validator checks the candidate's body in place of the original's.
4. **Equivalence.** The interpreter runs the original and the candidate on the same inputs. The
   candidate must end the same way (return the same words, revert or return the same data,
   stop, or reach `invalid`), write only memory bytes the original writes, and, when the
   original returns, leave those bytes the same. When the original ends without reverting, the
   candidate must also write exactly the storage and transient storage slots the original writes,
   leave each with the same value, and log the same events in the same order. A write fails in a
   static call whatever it stores, so a write the original does not make would fail where the
   original succeeds, and one it drops, even of the value a slot held, would succeed where the
   original fails. An input on which the original runs out of fuel runs again with twenty times
   as much; where the original still runs on, or calls too deeply, what it would do is unknown,
   so the candidate must run on as well, and one that ends there fails. A formula replaces a loop
   only when every input keeps the loop within the fuel, as a masked bound does. The inputs must
   exercise the candidate as they exercise the original, under the rules above. Decisions are
   comparisons and the `and`, `or`, and `xor` of booleans, which is how if-converted code
   combines comparisons without branching; values must come out nonzero because a path the
   inputs only complete on null pointers or empty data computes nothing a change would alter.
5. **Cost.** The target cost model prices the candidate, which must beat the best so far: by at
   least one stack copy of lifetime gas in gas builds, and in bytes, then gas, in size builds.

The adopted function keeps the original's name, signature, attributes, spans, and memory layout.
Its metadata is empty, and its debug information is marked as intentionally dropped.

### Inputs

Inputs come from a seed derived from the candidate text, so a rerun makes the same decisions.
Constants come from the function and every function it can call, with their neighbors and
left-aligned forms for `bytesN` comparisons. Arguments mix zero; powers of two, their neighbors,
and their negations; constants; small numbers; addresses near the free memory pointer; repeated
arguments to exercise aliasing; and random words, masked to their types. Memory holds seeded
garbage, except for the zero word at `0x60`, a free memory pointer from `0x80` up, which is
sometimes unaligned, and the objects at pointer arguments: a small length, which keeps loops over
memory short, then flags and constants. Garbage words are random, or small numbers, addresses
near the heap start, and constants, so that a function following pointers through memory reaches
nested objects.

Each input also has a world derived from its seed, which answers storage, transient storage, and
context reads: zero, a small number, a constant, one of the input's arguments, the caller, or a
random word, cut to the width the value has on chain, such as 160 bits for an address or 64 for
a timestamp. Only addresses keep their width everywhere: the EVM bounds no other value, and the
compiler relies on no such bound, so a quarter of the worlds, and every probe, answer the rest
with whole words. Each read draws from a hash of the seed and the whole slot or operands, so no
two slots share their draws. Transient storage is zero more often, as every transaction starts it
empty. A slot reads the same value throughout a run, and writes are kept until the run ends.

Probes then put each constant in each argument and, for functions that use storage or their
context, make each constant the world's answer to every read, or to half of them. A candidate that
adds a constant is also run with it in its arguments and its world.

Memory writes must stay within the original's because the backend keeps call frames and spill
slots in memory no function addresses, and callers may keep scratch words across a call. A
candidate that uses scratch space its original does not is rejected, even when it restores it.

### Cost model

A function costs the bytes of its reachable code and the average gas of the calls on inputs its
original returns on, including callees and memory growth. Probes, and inputs that grow memory by
more than 64 KiB because an argument acts as a far pointer, exercise the code but do not price
it. Each operation is priced by the target cost model, with dynamic work sized from the values
the run computed. Storage accesses are priced by the slot's state: an `SLOAD` or `SSTORE` is cold
at a run's first access to its slot and warm after, and an `SSTORE` costs what the value the slot
held when the run began, holds now, and receives make it cost, without refunds. Each operand
costs a push for an immediate or one stack copy otherwise, which stands in for stack scheduling.
An internal call costs the call protocol, except that a call to a function returning nothing,
which ends a function returning nothing, costs a jump, as the backend forwards it.
Accepted rewrites can still lose to the stack scheduler, so compare them with the runtime
benchmarks.

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
returns the next candidate or `Proposal::Done`; `LlmSession::finish` hears the verdict the last
candidate got, when no proposal heard it, and the cost of the rewrite the pass keeps.
`LlmRewriter::cached` hears about a function whose cached rewrite the pass keeps instead of asking. The command line's rewriter in
`crates/cli/src/llm.rs` is one such implementation, which the command line and
`solar::cli::standard_json::compile_standard_json` install only when no rewriter is installed.

An embedder can also keep that rewriter and carry its requests instead. With the `llm` feature,
`solar::cli::llm::set_transport` installs a `ChatTransport` that sends every request of the chat
providers, `anthropic/`, `opencode/`, and `openai-chat/`, in place of the compiler's client, and
the compiler then reads no key and sends none. `ChatTransport::send` takes a `reqwest::Request`
with a buffered body and returns the response; a `TransportError` it returns is sent again when
it is transient, and ends the turn otherwise. The CLI crate's `llm-transport` feature builds only
this path, without nanocodex, TLS, or a client of the compiler's own: `live` then requires a
transport, and `openai/` models, which only nanocodex asks, are unavailable.

A transport can pay for requests. A gateway that fronts a provider and charges per request with
the [Machine Payments Protocol](https://mpp.dev) answers an unpaid request with HTTP 402 and a
payment challenge, which the transport pays, for example from its user's wallet, before sending
the request again; `-Zllm-endpoint` names the gateway's API base, such as
`https://gateway.example/anthropic/v1` for an Anthropic route. Without a transport that pays, a
402 ends the turn. Foundry's `forge optimize` installs such a transport, paid from the Tempo
account its user signed in with.

An embedder that resolves a project's sources itself compiles them with
`solar::cli::run_compiler_with_sources`, which runs the command line's pipeline, outputs, and
rewriter in the embedder's session on the sources a closure adds to the parsing context; the
closure can also set the context's import remappings and include paths. `forge optimize`
compiles a Foundry project this way.

## Limits

- Testing is not proof. A candidate that differs only on inputs no generator reaches is
  accepted. Over the fifteen project archives of the runtime benchmark corpus, four single-site
  mutants of each of the 3,875 offered functions (a changed operation, operand order, constant,
  or branch, or a deleted memory store, storage write, or event) were checked: 15,200 of 15,348
  fail equivalence. The 148 that pass are equivalent, such as stores another store overwrites,
  scratch words a callee already wrote, the offset of an empty log or revert, or the low bytes of
  a four-byte error selector, or change behavior only on inputs the generators rarely reach,
  such as a bound one exact value reaches or a stored string shrinking to exactly one word.
  Proving loop-free candidates with the SMT checker in `scripts/evm-rules/` is future work.
- Inputs on which the original runs past 400,000 steps are not compared: the candidate only has
  to run as long there too. A candidate that runs as long but ends differently on such inputs
  passes.
- The interpreter models no calldata, `gas`, code reads, or calls to other contracts, so functions
  that use them are not offered. On the project archives, 28% of the reachable internal functions
  are offered. Decisions the inputs never complete both ways without reverting account for most
  of the rest, chiefly the allocator's wraparound check, which no free memory pointer the inputs
  choose trips, followed by `gas`, calldata, paths the tests do not complete, functions over 256
  instructions, and several return values.
- The cost model sees one stack copy per operand, not the stack scheduler's decisions.
- `live` sends function MIR to the rewriter's provider.
