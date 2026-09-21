# EVM rewrite placement and scalar cleanup

Keep scalar expression alternatives in MIR's acyclic e-graph. Keep physical
stack cleanup in EVM IR, where the costs of DUP, SWAP, spills, and block layout
are explicit. An EVM IR e-graph would need to lift stack code into values, model
effect dependencies, and schedule its extracted expressions back onto the stack.
Opcode-only extraction costs would miss the cost of sharing or extending live
ranges. This would repeat much of the existing MIR-to-EVM scheduler.

Use dependency tracking for late rewrites that can preserve the scheduled code.
`block-cse` already interns expressions and tracks memory/storage epochs;
`word-cleanup` tracks producer widths and consumer bit demand. They answer
different questions: equality of expressions versus which bits a use observes.
Neither requires another equality-saturation engine. Bounded ISLE peepholes
remain useful for adjacent instruction and stack-permutation identities.

## Compiler comparisons

Cranelift's [acyclic e-graph](https://cfallin.org/blog/2026/04/09/aegraph/)
combines canonicalization and expression alternatives before elaborating values
back into scheduled SSA. Its separation of pure expressions from the effectful
CFG supports Solar's MIR design; it does not make extraction from an already
scheduled physical stack free.

LLVM's [SelectionDAG](https://llvm.org/docs/CodeGenerator.html#selectiondag-instruction-selection-process)
combines expressions before scheduling and emission. Its
[known-bits and demanded-bits reasoning](https://llvm.org/doxygen/classllvm_1_1TargetLowering.html)
separates producer guarantees from consumer requirements. A clean producer makes
a mask redundant for every use; a truncating consumer only makes it redundant
for that use. Shared values need the union of all uses' requirements.

Solidity's legacy assembler uses
[KnownState](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libevmasm/KnownState.h)
and
[expression classes](https://github.com/argotorg/solidity/blob/8a079791d9cca7a6c03fd6a8429b93aa3bddefed/libevmasm/ExpressionClasses.cpp)
to track symbolic stack values, including storage/memory state sequence numbers.
Its CSE regenerates code from those expressions. Solar already uses a smaller
version of this approach in block CSE. Reuse state facts where they remove
concrete repeated work; broad late rescheduling needs separate cost evidence.

Vyper's [Venom CSE](https://github.com/vyperlang/vyper/blob/9b35e492b7e016f5bf9e1f907121391890ac8444/vyper/venom/passes/common_subexpression_elimination.py)
uses available-expression analysis with dataflow and liveness analyses. It
invalidates those analyses after replacement and repeats until no replacements
remain. This is another example of explicit dependency analysis for CSE,
without requiring an e-graph for the pass.

## Types and boundaries

A MIR `i160` value is a clean address. CALLER, ADDRESS, ORIGIN, COINBASE, and
creation operations produce that type. Widening it to `i256` preserves its
known upper zero bits and emits no cleanup. An exact round trip back to the
original type can reuse the original value during MIR construction. Pointer
round trips may do the same only when no bits were lost and the original type
matches exactly.

Solidity source types alone do not prove cleanliness. Yul can write high bits
into an address or a noncanonical word into a bool, then read those raw bits.
Internal arguments and returns deliberately preserve raw carriers where those
observations are legal. Keep dirty-value tracking for these boundaries and
normalize when Solidity semantics require it. ABI validation must still reject
invalid external arguments; it is not interchangeable with truncation.

Existing MIR rules already remove address cleanup at BALANCE and call-family
consumers when the cleaned value has one use. Those guards avoid extending live
ranges before scheduling. The late dependency pass covers shared values when
all uses truncate, and dependencies separated by stack operations, without
rescheduling them or extending a virtual value's live range.

The backend opcode schema declares result widths and implicitly truncated
operands. MIR width inference reuses result widths instead of keeping another
list of clean opcodes. EVM IR uses both contracts: address consumers observe
160 low bits, while MSTORE8 observes eight value bits and the entire offset.
Calls observe the full gas operand and only 160 bits of the address operand.

`word-cleanup` follows physical stack identities through DUP, SWAP, and EXCHANGE.
It propagates demand backwards through bitwise and modular arithmetic, removes
only redundant literal PUSH/AND pairs, and keeps masks needed by any full-width
observer. Unknown effects and protected bundles break tracking. Values that
leave a block are fully observed. Shifts, comparisons, and other unsupported
transfer functions conservatively demand all input bits. There is no alias
inference, inter-block bit analysis, or instruction motion in this pass.

Mask removal must not invalidate its own proof. Two masked operands cannot each
borrow the other's width bound and then both lose their masks. Backward AND
demand therefore uses only literal bounds, and a removed mask forwards the
consumer's demand unchanged to its source. This also keeps an inner mask when
it supplies the width guarantee used to remove an outer mask.
