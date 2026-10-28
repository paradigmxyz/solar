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
| `solar-inline` | a function that internal calls reach | Inline every internal call of it where inlining is legal. |
| `solar-noinline` | a function that internal calls reach | Never inline it. |
| `solar-optimize gas` or `size` | a non-abstract contract or a library | Optimize its code for this objective instead of the build's. |
| `solar-scratch` | a `{ ... }` or `unchecked { ... }` block | Later code reuses the memory the block allocates. |

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

## `solar-inline` and `solar-noinline`

```solidity
/// @custom:solar-inline
function mix(uint256 a, uint256 b) internal pure returns (uint256) {
    return a ^ (b >> 3);
}
```

Both document an internal, private, or public function with a body. `solar-inline` clones the
function into each caller where inlining is legal, whatever it costs, past the inliner's growth and
stack budgets. A contract takes at most about 16,000 instructions from such clones, so tagged
functions that call each other cannot multiply without limit; past that, a tagged function is
inlined only where an untagged one would be. `solar-noinline` keeps every call a call. A function
cannot have both, and builds with `-O none` inline nothing.

## `solar-optimize`

```solidity
/// @custom:solar-optimize size
contract Factory { ... }
```

Optimized builds compile the contract's code for the named objective, `gas` or `size`, instead of
the one the build selects. Builds with `-O none` stay unoptimized. The tag names exactly one
objective, and a contract has at most one.

## `solar-scratch`

```solidity
bytes32 digest;
/// @custom:solar-scratch
{
    bytes memory encoded = abi.encode(a, b);
    digest = keccak256(encoded);
}
```

The compiler reads the free memory pointer when the block starts and writes it back when the
block ends normally, so later allocations reuse the block's memory. A `return`, `break`, or
`continue` out of the block keeps the memory allocated. The block cannot contain inline
assembly, nor run a function body with it through a modifier's `_`.

Some temporary ABI buffers already reuse memory automatically. The tag extends reuse to a whole
block, including allocations in its callees; the compiler does not infer that scope yet.

Reuse is only correct while nothing after the block still refers to that memory, so the compiler
rejects every reference to it that later code could reach: a variable used after the block, a
`return`, a store into memory the block did not allocate or into storage, and a call whose callee
may store such a reference where later code can reach it. Values such as hashes, lengths, and words
read from the memory leave the block freely. After the block, the free memory pointer can be lower
than without the tag.

Assembly outside the block is allowed. In assembly functions, a memory load at an unknown address
may read the free memory pointer. Typed helpers can rely on valid object bounds when all assembly
explicitly promises memory safety and no function-pointer dispatcher reinterprets references.
Otherwise, unknown loads remain conservative throughout the contract. The compiler tracks possible
free-pointer reads through helper returns and stores too, and rejects the tag if the pointer may
outlive the block. Use a provably disjoint address, consume the pointer before the block, or read it
again after the block.
Copies and calls that may retain bytes from the free-pointer slot are captures too, even if no
instruction loads that slot directly. Storing a scalar into the slot and reading it back still
produces a free pointer; store forwarding does not exempt that read from the lifetime check.
The same check follows values installed into the free-pointer slot, even if code keeps the
original value without loading the slot again. Partial writes and copies that may install an
unknown pointer prevent reuse. Writes through helper parameters remain allowed when the caller
passes a fresh object that cannot reach the slot.
