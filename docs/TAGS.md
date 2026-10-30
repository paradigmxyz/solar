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
| `solar-fuse <group>` | mapping state variables with one key type | Keep the group's values for each key in one packed record. |
| `solar-inline` | a dynamic storage array state variable | Keep short arrays and their length in the array's own slot. |
| `solar-bitmap` | a mapping from an unsigned integer to `bool` | Pack 256 consecutive keys into one storage word. |
| `solar-handle <field> <dictionary>` | a struct declared in a contract | Store a dictionary index in place of the named field's value. |

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
  elimination, and the cost models they use, run on the function for its objective and skip it
  when its objective does not run them.
- A call is inlined by the objective of its caller. A function cloned into a caller with another
  objective joins the caller's code, so the caller's objective optimizes the clone, whatever the
  objective of the function it came from.
- The code of an external function's ABI wrapper and the lowering of its source.
- The backend's stack scheduling of the function and its choice of constants' encodings. In a
  contract built optimized, the EVM IR decisions that price a block, such as stack rescheduling
  and the priced part of tail merging, use it too.

What stays with the objective of the contract, since it concerns several functions at once:

- The dispatcher, the helpers that several functions share (ABI encoders and decoders, revert
  stubs, memory allocation and copy helpers), and the layout of immutables.
- The passes that rewrite functions together: specialization, function-pointer specialization,
  dead-argument elimination, merging of equivalent functions, revert outlining, no-return call
  ends, call, element and allocation cleanup, read-only evaluation, deferred allocation, and
  immutable check elimination. They run when the objective of the contract runs them.
- The EVM IR passes of the module as a whole, such as block layout, outlining, and tail
  duplication, and the assembler's jump table packing. The contract's objective enables them.

So in a contract built unoptimized, a tagged function gets the function-local MIR passes,
inlining into its code, and its stack scheduling and constant encodings, but not the
contract-wide passes above.

## Storage layout tags

These tags preserve Solidity-level reads and writes but change the raw storage layout. Other
compilers ignore them and use the standard layout. Do not upgrade between the two layouts or
share storage between contracts compiled with different layouts without a migration.

`solar-fuse <group>` groups at least two mappings of the same contract with the same key type
and value-type values. Their values for one key pack like a struct, in declaration order.
`solar-bitmap` instead packs a mapping's boolean values into bits. Both kinds of mapping may
only be indexed; storage references and assembly access to their slots are rejected.

A bare `solar-inline` on a dynamic storage array has a different meaning from the function tag:
it keeps the length and elements in the array's own slot while they fit, then uses the standard
array layout when it grows. Elements must be value types narrower than a word. Indexing,
length, push, pop, delete and copies into memory are supported; storage references, whole-array
assignment and assembly access to its slot are rejected. `always` and `never` apply only to
functions, not arrays.

`solar-handle <field> <dictionary>` replaces a word-sized value-type field with a nine-byte
handle: the dictionary index plus one, or zero for the zero value. The dictionary is a dynamic
storage array of the field's type in the same contract. It may only grow, and the field may
only be assigned a dictionary element or zero. Structs with handles cannot be assigned to
storage as a whole. Reads resolve the handle through the dictionary, so reading one field can
cost an extra storage read. The storage-layout output describes all four layouts.
