# NatSpec Tags

`@custom:solar-*` NatSpec tags state requirements the compiler checks, or let it compile code
differently without changing what the code computes. Other compilers read them as ordinary
`@custom:` documentation, so a tagged source still builds with `solc` and behaves the same there.

The whole `solar-` namespace is reserved. An unknown `@custom:solar-` tag, or a tag on something it
cannot document, is an error rather than a requirement that silently goes unchecked. The
divergence entry is [TYPECK-022](SOLC_DIVERGENCE.md#typeck-022-customsolar--tags-are-requirements).

| Tag | Documents | Meaning |
|---|---|---|
| `solar-safe [memory] [arithmetic]` | a contract or a library | No inline assembly by default; checked arithmetic is an optional requirement. |
| `solar-trusted` | a function, a modifier, a contract, or a library | Reviewed code that `solar-safe` takes on trust. |
| `solar-inline [always\|never]` | a function that internal calls reach | Favor inlining it, always inline it, or never inline it. |
| `solar-optimize gas` or `size` | a non-abstract contract, a library, or a function with a body | Optimize its code for this objective instead of the build's, even in an `-O none` build. |

## `solar-safe` and `solar-trusted`

```solidity
/// @custom:solar-safe memory arithmetic
library Parser {
    /// @custom:solar-trusted
    function wordAt(bytes memory b, uint256 i) internal pure returns (bytes32 w) {
        assembly ("memory-safe") {
            w := mload(add(add(b, 0x20), i))
        }
    }
}
```

`memory` requires that the code has no inline assembly, the only way Solidity code reaches memory
outside the objects it allocates. `arithmetic` requires that all of its arithmetic is checked: no
`unchecked` block, and no inline assembly, whose arithmetic wraps. A tag that names neither
requires only `memory`, so ordinary `solar-safe` code can use `unchecked`. The `arithmetic`
property remains an explicit requirement for code that must use checked arithmetic.

The code a contract runs is everything its creation and its entry points reach through internal
calls in its own call frame: functions of its bases, libraries and free functions, modifiers, base
constructors, state variable initializers, the override a virtual call dispatches to, and every
function whose value is taken. External calls, calls to deployed libraries and contract creations
run in frames of their own and are not part of it. A library's code is all of its functions.

`solar-trusted` marks reviewed code: the check does not look inside it, or into what only it
reaches. A call in trusted code that dispatches to an override or a `super` target is still
checked, since the contract chose that code, not the review.

## `solar-inline`

```solidity
/// @custom:solar-inline always
function mix(uint256 a, uint256 b) internal pure returns (uint256) {
    return a ^ (b >> 3);
}
```

The tag documents an internal, private, or public function with a body, appears once, and takes
no argument, `always`, or `never`. Inlining happens only in optimized code, and the inliner prices
each call by the objective of the function that makes it.

- `solar-inline always` clones the function into each caller where inlining is legal, whatever it
  costs, past the inliner's growth and stack budgets. A contract takes at most about 16,000
  instructions and blocks from such clones, so tagged functions that call each other cannot
  multiply without limit; past that, a tagged function is inlined only where an untagged one
  would be.
- `solar-inline never` keeps every call a call.
- A bare `solar-inline` is a hint, as `#[inline]` is next to `#[inline(always)]`. The inliner
  favors the function: it clones it wherever inlining is legal and the clone adds at most as many
  bytes again as the call it replaces under the gas objective, or no bytes under the size
  objective, without weighing the gas it saves or the shape of its body. The bound is the
  target's cost model (`Target::inline_hint_admits`). The legality, stack, and caller growth
  budgets still apply, so the hint never inlines a call that the inliner may not.

## `solar-optimize`

```solidity
/// @custom:solar-optimize size
contract Factory { ... }

contract Token {
    /// @custom:solar-optimize gas
    function transfer(address to, uint256 amount) public returns (bool) { ... }
}
```

The tag names exactly one objective, `gas` or `size`, and an item has at most one. It optimizes
the code of the contract, the library, or the function it documents for that objective instead of
the one the build selects. It also optimizes when the build is `-O none`: a tagged contract
compiles to the bytecode of a build for its objective, and the rest of the build stays
unoptimized.

A function with the tag is a function with a body that is not a constructor, a modifier, or a
`receive` or `fallback` function; it may be internal, private, public, or external. Its code is
optimized for its objective where a decision concerns that function alone. A tag that names the
objective its contract already has changes nothing.

What follows a function's objective:

- The MIR passes that rewrite one function at a time, such as loop unrolling, hoisting, check
  elimination, and the stack-based cost models they use, run on the function for its objective
  and skip it when its objective does not run them.
- A call is inlined by the objective of its caller. A function cloned into a caller with another
  objective joins the caller's code, so the caller's objective optimizes the clone, whatever the
  objective of the function it came from.
- The code of an external function's ABI wrapper, the lowering of its source, and its stack
  scheduling, block layout, and the EVM IR decisions that price a block (stack rescheduling, tail
  merging, push compaction) in the backend.

What stays with the objective of the contract, since it concerns several functions at once:

- The dispatcher, the helpers that several functions share (ABI encoders and decoders, revert
  stubs, memory allocation and copy helpers), and the layout of immutables.
- The passes that rewrite functions together: specialization, dead-argument elimination,
  merging of equivalent functions, revert outlining, call, element and allocation cleanup,
  read-only evaluation, and deferred allocation. They run when the objective of the contract runs
  them.
- The EVM IR passes of the module as a whole, such as block layout, outlining, and tail
  duplication, and the assembler's jump table packing. The contract's objective enables them.

So in a contract built unoptimized, a tagged function gets the function-local MIR passes,
inlining into its code, and its backend decisions, but not the contract-wide passes above.
