# Safe Solidity: the core library and Solar tags

Libraries such as Solady reach for inline assembly whenever Solidity cannot say
something, or cannot say it cheaply: read a word out of `bytes`, copy a range
without a loop, return from inside a helper, bound the data a call copies back,
deploy raw initcode, or reuse memory. That assembly is unchecked by
construction. This document describes how the same code can be written in
checked Solidity that any compiler still compiles, and how this compiler removes
the overhead that checked code would otherwise pay:

- **Core modules**: the primitive operations live in compiler-owned libraries
  imported from `solar:core/v1/...`. Their bodies are portable Solidity that
  other compilers compile as written; this compiler recognizes the functions by
  module identity and lowers them directly.
- **Solar tags**: NatSpec tags of the form `@custom:solar-*` state requirements
  on ordinary code, such as reading a range in place or keeping a contract free
  of assembly. This compiler checks them; other compilers read them as
  documentation.

[Safe Solady](#safe-solady) applies both to Solady and measures the result
against the original assembly.

## Principles

1. **The source stays Solidity.** There is no new syntax. A program this
   compiler accepts behaves the same under solc; only gas and code size differ.
   That is the rule for every module and tag; the compiler's own known
   divergences from solc are recorded in [SOLC_DIVERGENCE.md](SOLC_DIVERGENCE.md).
2. **The compiler owns the primitives.** Operations with no checked spelling
   are functions of core modules, identified by the module they come from, not
   by their names. Their failures are checked and defined.
3. **A tag is a requirement, never a hint.** A tag lets the compiler do
   something cheaper, such as reading bytes where they lie instead of copying
   them, or it states a property of the code. The compiler proves what the tag
   needs or rejects the program with a diagnostic. It never compiles a tagged
   program to different behavior.

## Core modules

### Importing

```solidity
import {Bytes} from "solar:core/v1/Bytes.sol";
import {Buffers, ByteBuilder} from "solar:core/v1/Buffers.sol";
```

The `solar:core/` prefix is reserved. An import under it resolves to source
embedded in the compiler before any file resolution, so no remapping or file on
disk can stand in for a module. A standard JSON source under a reserved name is
set aside: an exact copy of the module is accepted silently, and different
content is reported with a warning and ignored. The `v1` in the path versions
the API.

### Portable bodies and intrinsics

Every module function has a body that any Solidity compiler accepts, and that
body defines the operation, including how it fails. For most entry points this
compiler does not compile the body: it lowers the call directly, by module
identity, to the word operations, copies, calls or terminators the body stands
for. `-Zno-core-intrinsics` compiles the bodies instead, which is how the two
are compared.

A few operations have no portable Solidity spelling at all, such as shortening
an array in place, reverting with raw bytes, or deploying initcode. Their bodies
are one memory-safe assembly block each. The modules are the trusted primitive
layer: `@custom:solar-safe` and the Safe Solady audit do not check their bodies.

### Using the modules with solc and Foundry

`solar export-core <dir>` writes every module under `<dir>` at its import path,
so sources import them unchanged:

```sh
solar export-core core
solc --base-path . --include-path core src/Token.sol
```

Foundry resolves a tree written at the project root (`solar export-core .`). A
remapping cannot supply the modules, because remapping syntax reads the colon as
a context separator.

### Build provenance

A module's code comes from the compiler, so an import path does not pin a
build. The standard JSON output `solarBuild`, selected by its exact name in
`outputSelection`, records what a contract's bytecode depends on besides its
sources: the compiler version and commit, the EVM version, the optimization
settings, whether module functions are lowered directly (`coreIntrinsics`), and
the keccak256 of every module the contract's source reaches.

### Failures

Module functions fail the way checked Solidity fails, before they write
anything:

| Failure | Raised by |
|---|---|
| `Panic(0x32)` | a read, write, copy, fill or slice outside a buffer; a `Slots` index at or above `2**64` |
| `Panic(0x11)` | a narrowing cast that does not fit; a `mulDiv` quotient that does not fit |
| `Panic(0x12)` | `mulDiv` by zero |
| `DeploymentFailed()` | `Create.deploy` and `deploy2` |
| `InvalidBase64()`, `InvalidHex()` | the strict decoders |

The `try` forms, such as `Bytes.tryReadBytes32` and `Create.tryDeploy`, report
the failure instead.

### Module reference

| Module | Provides |
|---|---|
| `Bytes.sol` | `readBytes1` to `readBytes32`, `readUint256BE`, the matching `write*` and `tryRead*`, `copyInto` (a move, safe for overlapping ranges), `fill`, `equals`, `equalsAt`, and `slice` (a copy, or a view under `@custom:solar-view`) over `bytes memory` |
| `CalldataBytes.sol` | the same reads and `copyInto` over `bytes calldata` |
| `Arrays.sol` | `truncate(a, n)` for every dynamic memory array type: shrink-only, and every alias sees the new length |
| `WordArrays.sol` | `sort`, `uniquifySorted`, `hasDuplicate`, `union`, `intersection`, `difference`, `groupSum` and `copy` over `uint256[]`, `int256[]`, `address[]` and `bytes32[]` |
| `Buffers.sol` | builders for output of unknown length: `ByteBuilder`, `WordBuilder`, `AddressBuilder`, `Bytes32Builder` and `Int256Builder`, with `create`, `append`, `appendByte`, `length` and `finish` |
| `Strings.sol` | `toString`, the hex strings, `indexOf`, `lastIndexOf`, `indicesOf`, `split`, `replace`, `repeat`, `equals`, `escapeJSON`, `escapeHTML`, `encodeURIComponent`, `isValidUTF8`, `runeCount`, and small-string packing (`packOne`, `packTwo`, `unpackOne`, `unpackTwo`) |
| `codecs/Base64.sol`, `codecs/Hex.sol` | `encode` and strict `decode` |
| `Abi.sol` | `encodeInto` and `encodeSelectorInto` a buffer the caller owns, `writeEncoding(out, offset, abi.encode(...))`, `encodedSize(abi.encode(...))` without allocating the encoding, and `fits` |
| `Hash.sol` | `keccak256Range(b, offset, count)`, hashed where it lies |
| `Calls.sol` | `callInto`, `staticCallInto` and `delegateCallInto` into the caller's buffer; `callBounded` and `staticCallBounded`, which copy at most `maxCopy` bytes of the response; `forward` and `forwardDelegate`, which end the call with the callee's response |
| `Create.sol` | `deploy`, `deploy2`, `tryDeploy`, `tryDeploy2`, `tryDeployInto` and `predict2` for initcode built at runtime |
| `Code.sol` | `read` and `copyInto` of a range of another account's code, and `slice`, which makes a `CodeView` range (with `account`, `offset` and `length`) that every read checks against the code again |
| `Revert.sol` | `raw(data)`: revert with exactly `data`, to bubble another call's revert |
| `Return.sol` | `abiEncoded(value)` and `raw(data)`: end the whole call successfully from any function |
| `Slots.sol` | storage words derived from a `Slots.Root`, laid out like a dynamic array's elements: `load`, `store`, `storeBytes`, `storeCalldataBytes` and `loadBytes` |
| `Math.sol` | `mul512`, `mulDiv(x, y, d, Rounding)`, `sqrt`, and `wrappingAdd`, `wrappingSub` and `wrappingMul` for arithmetic that means to wrap |
| `Bits.sol` | `leadingZeros`, `highestSetBit`, `trailingZeros` and `popCount`, returning 256 for zero |
| `Cast.sol` | `toUint8` to `toUint256` and `toInt8` to `toInt256`, reverting instead of truncating |
| `Precompiles.sol` | `ecAdd`, `ecMul`, `ecPairing`, `modexp`, `blake2f`, `p256Verify`, `pointEvaluation` and the BLS12-381 operations, each reporting success separately and checking the response size |
| `Build.sol` | `gasFirst()`, true in builds optimized for gas, so a library can keep a path that only pays for itself in gas out of other builds |

The modules' own documentation comments give the exact contract of each
function; `solar export-core` writes them out for reading.

### Checks tied to the modules

Some rules need no tag, because they follow from using a module:

- **Builders.** A builder's fields belong to `Buffers`: code outside the module
  that reads or writes them, or makes a builder from parts, is rejected, and so
  is any use of a builder after `finish`. A builder cannot be part of another
  type. Since no code can read a builder's unwritten capacity, the compiler does
  not zero it.
- **Code views.** Only `Code` makes or unpacks a `CodeView`, so a range that no
  one checked cannot exist.
- **Successful exits.** `Return.abiEncoded`, `Return.raw`, `Calls.forward` and
  `Calls.forwardDelegate` end the external call from any depth, so the compiler
  checks every call to them against every entry point that can reach it:
  creation may reach none, raw bytes may only end a call to the fallback
  function, and `Return.abiEncoded(value)` may end a call to the fallback or to
  a function that returns exactly one value of the same ABI type. None of them
  may run while a modifier's code after `_` is still pending, since returning
  would skip it. A literal converts to more than one overload, so it names its
  type: `Return.abiEncoded(string("done"))`.

## Solar tags

A Solar tag is a NatSpec custom tag, written on its own doc-comment line:

```solidity
/// @custom:solar-view
bytes memory head = packet.slice(0, 4);
```

solc reads `@custom:` tags as documentation: it accepts them on statements and
records the ones on contracts and functions in its `devdoc` output. The whole
`solar-` namespace is reserved: an unknown tag or a tag in the wrong place is an
error, so a misspelled requirement never goes unchecked. A doc-comment line that
starts with a tag is a tag for both compilers; a mention in the middle of a line
is prose.

| Tag | Documents | Requires |
|---|---|---|
| `@custom:solar-view` | a variable declaration, or an internal or private function naming its parameters | the declared memory references are read in place, with no copy |
| `@custom:solar-scratch` | a block statement | the memory the block allocates is reused after it |
| `@custom:solar-terminates` | an internal or private function with a body | the function ends the call on every path |
| `@custom:solar-safe` | a contract or a library | the code it runs has no inline assembly and no unchecked arithmetic |
| `@custom:solar-trusted` | a function, modifier, contract or library | nothing; it marks reviewed code that `@custom:solar-safe` does not look inside |

### `@custom:solar-view`

Without the tag, `Bytes.slice` and `abi.decode` copy. With it, the declared
variable is a view of the bytes where they are:

```solidity
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Packets {
    using Bytes for bytes;

    function header(bytes memory packet) external pure returns (bytes4 selector, bytes32 bodyHash) {
        /// @custom:solar-view
        bytes memory head = packet.slice(0, 4);
        selector = head.readBytes4(0);
        /// @custom:solar-view
        bytes memory body = packet.slice(4, packet.length - 4);
        bodyHash = keccak256(body);
    }
}
```

The tag can document:

- a `bytes memory` variable initialized by `Bytes.slice`;
- a declaration initialized by `abi.decode` of `bytes` in memory or calldata,
  whose memory references (`bytes`, `string`, arrays and structs) become views of
  the encoding. The decode validates its input exactly as the copying decode
  does, including the allocation checks, so every input fails where it would;
- a memory reference read from a view, such as `bytes memory first = items[0];`;
- an internal or private function, naming view parameters:
  `/// @custom:solar-view data`. The function reads `data` in place, and
  callers pass what they hold without a copy.

A view is equivalent to a copy only while its bytes cannot change and nothing
can tell the two apart, and the compiler checks both. It rejects a write that
may reach bytes a live view still reads:

```text
error: this may change bytes that the view `head` still reads
```

and any use other than reading in place: `.length`, indexing and field reads,
the hashes, `abi.decode`, the ABI encodings and concatenations, event, error and
external call arguments, the `Bytes` and `Hash` range reads, and passing it as a
view parameter. Assigning, returning, storing or passing a view elsewhere is an
error at that use:

```text
error: the view `head` can only be read in place
```

A view of calldata needs no borrow check, since nothing can change calldata.

### `@custom:solar-scratch`

```solidity
bytes32 digest;
/// @custom:solar-scratch
{
    bytes memory encoded = abi.encode(a, b);
    digest = keccak256(encoded);
}
```

The compiler reads the free memory pointer when the block starts and restores it
when the block ends, so later allocations reuse the block's memory. It proves
that no reference to that memory survives the block: a use after the block, a
`return` of one, a store of one into older memory or storage, or a call that may
keep one where later code can reach it is rejected. Scalars such as hashes,
lengths and loaded words leave the block freely. A block that `return`, `break`
or `continue` leaves early keeps its memory allocated, and inline assembly is
not allowed inside the block.

### `@custom:solar-terminates`

```solidity
import {Revert} from "solar:core/v1/Revert.sol";

/// @custom:solar-terminates
function _bubble(bytes memory reason) internal pure {
    Revert.raw(reason);
}
```

The function must end the call on every path: by reverting, by a halting
builtin, by a core module function that ends the call (`Revert.raw`,
`Return.abiEncoded`, `Return.raw`, `Calls.forward`, `Calls.forwardDelegate`),
or by calling another non-virtual `@custom:solar-terminates` function. The
check follows the body's structure: an `if` ends the call only when both
branches do, and loops and `try` never do. A tagged function takes no
modifiers, since a modifier could skip its body.

### `@custom:solar-safe` and `@custom:solar-trusted`

`@custom:solar-safe` on a contract or library turns two properties of the code
it runs into requirements:

- `memory`: no inline assembly, the only way Solidity reaches memory outside
  the objects it allocates;
- `arithmetic`: all arithmetic is checked, with no `unchecked` block, no call to
  `Math.wrappingAdd`, `wrappingSub` or `wrappingMul`, and no inline assembly.

`/// @custom:solar-safe memory` or `/// @custom:solar-safe arithmetic` requires
one property; the bare tag requires both. The code a contract runs is what its
creation and its entry points reach through internal calls: bases, libraries,
free functions, modifiers, base constructors, the overrides virtual calls
dispatch to, and every function whose value is taken. External calls, calls
to deployed libraries and contract creations run in call frames of their own
and are not included.

```text
error: `Bad` is tagged `@custom:solar-safe` but runs inline assembly
   ╭▸ src/Bad.sol:24:9
   ...
   ├ note: it runs this through `Bad.mix`
   ├ help: write it without assembly, or review it and tag its function `@custom:solar-trusted`
```

Two kinds of code are trusted: the core modules, and code tagged
`@custom:solar-trusted`, whose inside the profile does not check. Review, not
the compiler, answers for trusted code. The standard JSON output `solarSafety`
reports both properties for every contract, tagged or not, and lists the
trusted functions its code runs:

```json
"solarSafety": { "memory": true, "arithmetic": true, "trusted": ["Reviewed.word(bytes)"] }
```

### ERC-7201 namespaces

The standard `@custom:storage-location erc7201:<id>` annotation is not a Solar
tag, but the compiler checks the one part of namespaced storage the language
cannot type: the accessor that points a storage reference at the namespace in
inline assembly.

```solidity
/// @custom:storage-location erc7201:example.vault
struct VaultStorage {
    uint256 total;
}

// keccak256(abi.encode(uint256(keccak256("example.vault")) - 1)) & ~bytes32(uint256(0xff))
bytes32 private constant VAULT_LOCATION =
    0xd1921ee58d28820c9487d4d5d3eec1942edd7f5897e909e18a400cd2422da100;

function _vault() private pure returns (VaultStorage storage $) {
    assembly {
        $.slot := VAULT_LOCATION
    }
}
```

A constant assigned to `$.slot` must be the namespace's location, and the error
names the right one. A value the compiler cannot evaluate at compile time is not
checked, so write the location as a literal, as above. No contract may see two
structs in one namespace. Such an accessor needs no `@custom:solar-trusted`
under `@custom:solar-safe`, and the `storageLayout` output lists the namespaces
under `namespaces`.

## Porting assembly

| Assembly idiom | Checked replacement |
|---|---|
| `mload(add(add(b, 0x20), o))` | `Bytes.readBytes32(b, o)` or `readUint256BE` |
| `mstore(add(add(b, 0x20), o), w)` | `Bytes.writeBytes32(b, o, w)` |
| `calldataload(add(data.offset, o))` | `CalldataBytes.readBytes32(data, o)` |
| a copy loop or `mcopy` between buffers | `Bytes.copyInto(dst, dstOffset, src, srcOffset, count)` |
| `keccak256(add(add(b, 0x20), o), n)` | `Hash.keccak256Range(b, o, n)` |
| pointer arithmetic into a buffer | `Bytes.slice` under `@custom:solar-view` |
| `mstore(a, n)` to shorten an array | `Arrays.truncate(a, n)` |
| resetting the free memory pointer | a `@custom:solar-scratch` block |
| building output past the free memory pointer | `Buffers.create`, `append` and `finish` |
| `revert(add(data, 0x20), mload(data))` | `Revert.raw(data)` |
| `return(p, n)` from a helper | `Return.abiEncoded(value)` or `Return.raw(data)` |
| `call` with a bounded `returndatacopy` | `Calls.callBounded` or `staticCallBounded` |
| a proxy's `calldatacopy`, `delegatecall` and `return` | `Calls.forwardDelegate(target, msg.data)` |
| `create` or `create2` of built initcode | `Create.deploy` or `deploy2` |
| `extcodecopy` | `Code.read(target, start, count)` |
| slot arithmetic for data kept past a root word | `Slots.load`, `store`, `storeBytes` and `loadBytes` |
| `unchecked` modular arithmetic | `Math.wrappingAdd`, `wrappingSub` and `wrappingMul` |
| `staticcall` to a precompile | the `Precompiles` wrappers |

For most operations, `tests/ui/safe-solar/` holds a `safe.sol` written with the
modules next to an `unsafe.sol` with the assembly it replaces, and checks their
code and results.

## Safe Solady

[Safe Solady](https://github.com/djolertrk/solady/tree/feat/safe-core-lib) is a
fork of Solady v0.1.26 that rewrites its libraries without inline assembly and
without `unchecked` blocks, using only the core modules for the primitive
operations. It keeps each library's non-private declarations, custom errors and
events; every deliberate difference in behavior is written in the library's
header. The fork's [`SAFE_SOLADY.md`](https://github.com/djolertrk/solady/blob/feat/safe-core-lib/SAFE_SOLADY.md)
records every round of results.

How the port is held to that:

- an AST audit rejects `InlineAssembly` and `UncheckedBlock` nodes in every
  ported source and dependency outside `solar:core/`;
- a declaration audit compares every ported function's name, parameters,
  returns, visibility and mutability with upstream;
- six legs, the upstream source and the port, each under solc's legacy and IR
  pipelines and under this compiler, run the same calls against Python oracles;
  the reference is the cheaper solc pipeline on the upstream assembly, per call;
- composed workloads chain the libraries that call and deploy, and a stateful
  runner replays transactions on the ERC20 port against a model of upstream;
- the pinned upstream test suites run against the ported sources.

At the time of writing, fifteen sources are ported, with 508 of their 551
non-private declarations: SafeCastLib, LibBit, Base64, LibSort, SSTORE2,
LibCall, SafeTransferLib, LibClone, ECDSA, SignatureCheckerLib and ERC20 whole,
and LibString, EfficientHashLib, MerkleProofLib and LibBytes in part. Compiled by
this compiler, the port uses 0.53 times the gas of the cheaper solc pipeline on
the upstream assembly, summed over the comparable cases among 24,331 per-call
cases, and its ERC20 costs 1.13% less over a transaction sequence, both at 200
optimizer runs. Some calls still lose; `SAFE_SOLADY.md` lists them.

Run the comparisons from the fork's root, with `BENCH_SOLC` set to the solc
binary to compare against:

```sh
uv run benchmarks/checked/benchmark.py --solc "$BENCH_SOLC" \
  --solar ../solar/target/debug/solar --runs 200 --output target/safe-solady/run-200
uv run benchmarks/checked/composed.py --solc "$BENCH_SOLC" \
  --solar ../solar/target/debug/solar --output target/safe-solady/composed-200
uv run benchmarks/checked/erc20.py --solc "$BENCH_SOLC" \
  --solar ../solar/target/debug/solar --output target/safe-solady/erc20-200
uv run benchmarks/checked/upstream_tests.py --solc "$BENCH_SOLC" \
  --solar ../solar/target/debug/solar --output target/safe-solady/upstream-tests
```

## Implementation

- `crates/sema/src/core/`: the modules and their registry, including the
  entry points the compiler lowers directly.
- `crates/sema/src/natspec.rs`: tag names and placement.
- `crates/sema/src/typeck/`: `solar_tags.rs` (view shapes and view parameters,
  `@custom:solar-terminates`), `safe_profile.rs`, `call_exits.rs`,
  `builders.rs`, `code_views.rs` and `erc7201.rs`.
- `crates/codegen/src/mir/lower/function/`: `core.rs` lowers module calls,
  `views.rs` checks view borrows, `scratch.rs` checks scratch blocks, and
  `terminates.rs` checks exits under modifiers.
- `tests/ui/safe-solar/`, `tests/ui/typeck/solar_view_*.sol` and
  `tests/ui/standard-json/solar-safety/` and `solar-build/`.

## Limits

- A view can only be read in place. There is no way to return it, keep it, or
  choose between views; copy instead by removing the tag.
- There are no raw memory pointers. The free memory pointer cannot be rewound
  except by a `@custom:solar-scratch` block, which is why the port's
  `EfficientHashLib.free` does nothing.
- Storage is reached through ordinary variables, `Slots` regions and ERC-7201
  namespaces, never an arbitrary slot. A port whose original hashes its own
  slots, such as ERC20's balances, therefore has a different storage layout.
- No Solidity expression is an empty `bytes32[] calldata`, so MerkleProofLib's
  `emptyProof`, `emptyLeaves` and `emptyFlags` stay unported.
- Where the compiler cannot prove what a tag needs, it rejects the program even
  if the program is correct. Removing the tag always gives the plain Solidity
  meaning back.
