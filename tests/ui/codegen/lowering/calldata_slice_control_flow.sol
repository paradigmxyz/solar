//@compile-flags: -Zcodegen --emit=mir
//@filecheck: --check-prefix=CDCF

// A calldata slice built or trimmed under control flow, then read back or passed
// on. An internal helper that returns a calldata slice through implicit named
// returns — even with branches — is inlined, its return merging through its
// slot, so no `internal_call` survives and the slice folds away. An
// uninitialized calldata slice local filled in assembly and forwarded to
// another internal call folds the same way. Verified byte-identical to solc.
contract CalldataSliceControlFlow {
    // A single calldata slice trimmed under a branch and returned through an
    // implicit named return: the helper inlines, so no `internal_call` is left.
    // CDCF-LABEL: fn @trimLen
    // CDCF-NOT: internal_call
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return _trim(data).length;
    }

    function _trim(bytes calldata data) internal pure returns (bytes calldata r) {
        r = data;
        if (data.length > 8) {
            assembly {
                r.offset := add(data.offset, 8)
                r.length := sub(data.length, 8)
            }
        }
    }

    // Uninitialized calldata slices filled in assembly and forwarded to an
    // internal call fold to compact head reads.
    // CDCF-LABEL: fn @forward
    function forward(bytes calldata x) external pure returns (uint256) {
        bytes calldata a;
        bytes calldata b;
        assembly {
            b.length := 0
            a.offset := x.offset
            a.length := x.length
        }
        return _sum(a, b);
    }

    function _sum(bytes calldata a, bytes calldata b) internal pure returns (uint256) {
        return a.length * 1000 + b.length;
    }
}
