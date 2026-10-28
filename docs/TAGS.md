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
| `solar-terminates` | an internal or private function | Every path ends the call. |
| `solar-scratch` | a `{ ... }` or `unchecked { ... }` block | Later code reuses the memory the block allocates. |
| `solar-view` | a declaration initialized by `abi.decode` of calldata `bytes` | Its `bytes` and `string` values are read where they are, with no copy. |

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

## `solar-terminates`

```solidity
/// @custom:solar-terminates
function fail(uint256 code) internal pure {
    revert Failed(code);
}
```

The tag documents an internal or private function with a body and requires that every path
through it ends the call. A statement ends it when it reverts, calls a builtin that reverts or
halts, calls a non-virtual function with the tag, or is a block, or an `if` with an `else`, whose
parts all do. Loops and `try` never count, a `return` is an error, and the function cannot take
modifiers. The optimizer finds functions that cannot return on its own; the tag makes the
property a checked part of the function's contract.

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

Reuse is only correct while nothing after the block still refers to that memory, so the compiler
rejects every reference to it that later code could reach: a variable used after the block, a
`return`, a store into memory the block did not allocate or into storage, and a call whose callee
may store such a reference where later code can reach it. Values such as hashes, lengths, and words
read from the memory leave the block freely. After the block, the free memory pointer and `msize`
can be lower than without the tag.

Assembly outside the block is allowed, but a memory load at an unknown address may read the
free memory pointer. The compiler tracks that possibility through helper returns and stores
too, and rejects the tag if the pointer may outlive the block. Use a provably disjoint address,
consume the pointer before the block, or read it again after the block.
Copies and calls that may retain bytes from the free-pointer slot are captures too, even if no
instruction loads that slot directly. Storing a scalar into the slot and reading it back still
produces a free pointer; store forwarding does not exempt that read from the lifetime check.

## `solar-view`

```solidity
function handle(bytes calldata data) external {
    /// @custom:solar-view
    (uint256 id, bytes memory payload) = abi.decode(data, (uint256, bytes));
    bytes32 digest = keccak256(payload);
    bytes1 kind = payload[0];
    ...
}
```

Every compiler gives `payload` a copy of its bytes. With the tag, this compiler makes each `bytes`
and `string` variable of the declaration a view of its bytes where the decode finds them in
calldata, which cannot change. The decode validates its input exactly as the copying decode does,
including the checks of the allocations the copies would make, so every input fails where it
would: `Panic(0x41)` for a length that could not be allocated, and a revert for an offset or a
length past the input. The payload stays in calldata. In functions that decode a view again,
the copies' space is reserved so nested allocation checks account for earlier decodes. Other
view decodes leave the free memory pointer unchanged.

A view can only be read in place, so no use can tell it from the copy: its `.length`, an index
read, `keccak256`, `abi.decode`, and the encodings that copy their arguments into their output
(`abi.encode` and its variants, `abi.encodePacked`, `bytes.concat` and `string.concat`), also
through a conversion between `bytes` and `string`. A tagged `abi.decode` of a view makes views of
the same calldata. Any other use, such as an assignment, a write through the view, a return, a
call, event, or error argument, or inline assembly, is an error at the use. The decoded types may
be value types, `bytes`, and `string`, and the data must be `bytes` in calldata or a view.

