# solar-std

Solidity sources for the compiler's standard library.

The modules live under `solidity/`, at their import paths below the reserved
`solar:core/` prefix: `import {Bytes} from "solar:core/Bytes.sol";` reads
`solidity/Bytes.sol`. The compiler embeds the text and never reads these files.
A project that also builds with another compiler can remap `solar:core/` to a
copy of the directory; this compiler accepts such a copy only when its text is
exactly the module's.

`gen_modules.py` generates the modules whose sources say so.

Use the same sources with solc by remapping `solar:core/` to `solidity/`.
Compiler-owned names cannot be replaced by remappings or supplied source text.
Functions with the same names in ordinary user libraries remain ordinary calls.

The initial intrinsics cover memory/calldata reads and copies, memory writes,
array truncation, range hashing, full-width multiplication, wrapping arithmetic
and bit scans. Other operations use their Solidity bodies. The reviewed raw
operations and modular arithmetic are marked `solar-trusted`; callers can keep
using `solar-safe`. `-Zno-core-intrinsics` disables the special lowering.

```solidity
import {Bytes} from "solar:core/Bytes.sol";
import {Bits} from "solar:core/Bits.sol";

contract Example {
    function read(bytes memory input) external pure returns (bytes4, uint256) {
        return (Bytes.readBytes4(input, 0), Bits.leadingZeros(1));
    }
}
```

`Buffers` provides byte and word builders, `Abi` writes encodings into existing
buffers, and `Code` reads checked code ranges. Builder fields and `CodeView`
construction belong to their defining modules. The compiler checks builder
encapsulation, uses after `finish`, and return operations against each reachable
entry point, including ABI-only builds. Backing memory remains zero-initialized.

Builder lifetime checks follow helpers with their actual arguments, including
aliases, conditional finishes and reverting paths. Analysis is limited to 16,384
cloned instructions and blocks per entry, eight paths per block, and 64 scalar
facts and finished references per path. Beyond these limits it merges paths,
forgets scalar facts or rejects the analysis. Recursion it cannot prove safe is
rejected; complex entries may need to be split. The analysis never edits emitted
code. Assembly can hide builder references and is outside this check.

`Build.gasFirst()` follows the current function's objective, including
`solar-optimize` overrides, for direct and indirect calls alike. Its portable body
returns true.

`WordArrays`, `Strings`, `codecs/Base64` and `codecs/Hex` remain portable source
libraries. Algorithms are compiled through the normal optimization pipeline;
there are no baked sorting or codec kernels in the compiler. The byte-range
primitives still receive intrinsic lowering, with portable fallbacks for tests.

Substring searches use KMP for needles longer than 16 bytes and bounded byte
comparisons otherwise. Non-overlapping searches make two linear scans with one
prefix table to allocate exactly the result size. Sorting and paired grouping use
in-place heapsort above 16 elements; smaller inputs use insertion sort. Duplicate
checks cap hash probing at eight occupied slots before sorting a copy, so crafted
collisions cannot force quadratic work and the input stays unchanged. Signed
sorting uses signed order; signed grouping keeps raw word order.

For a Foundry project whose library imports live under `src/`, the solc remapping
is `src/:solar:core/=path/to/solidity/`. The explicit context preserves the colon
in the import prefix when Foundry serializes its remappings.
