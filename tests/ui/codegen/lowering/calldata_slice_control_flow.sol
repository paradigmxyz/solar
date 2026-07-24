//@compile-flags: -Zcodegen -Zdump=mir
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
    // CDCF-LABEL: fn @trimLen{{[( ]}}
    // CDCF-NOT: internal_call
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return _trim(data).length;
    }

    // CDCF-LABEL: fn @_trim{{[( ]}}
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
    // CDCF-LABEL: fn @forward{{[( ]}}
    // CDCF-NOT: internal_call
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

    // CDCF-LABEL: fn @_sum{{[( ]}}
    function _sum(bytes calldata a, bytes calldata b) internal pure returns (uint256) {
        return a.length * 1000 + b.length;
    }

    // An explicit `return` under control flow: the body inlines with an inline
    // exit block, each `return` storing to the return slot and jumping there.
    // CDCF-LABEL: fn @explicitTrim{{[( ]}}
    // CDCF-NOT: internal_call
    function explicitTrim(bytes calldata x) external pure returns (uint256) {
        return _explicitTrim(x).length;
    }

    // CDCF-LABEL: fn @_explicitTrim{{[( ]}}
    function _explicitTrim(bytes calldata x) internal pure returns (bytes calldata) {
        if (x.length > 4) return x[4:];
        return x;
    }

    // Destructuring a multi-slice return: the inlined callee delivers both
    // slices directly to the bindings, bypassing the one-word-per-value
    // multi-return buffer that cannot carry a two-word slice.
    // CDCF-LABEL: fn @headTail{{[( ]}}
    // CDCF-NOT: internal_call
    function headTail(bytes calldata x) external pure returns (uint256 hl, uint256 tl) {
        (bytes calldata head, bytes calldata tail) = _split(x);
        hl = head.length;
        tl = tail.length;
    }

    // CDCF-LABEL: fn @_split{{[( ]}}
    function _split(bytes calldata x)
        internal
        pure
        returns (bytes calldata head, bytes calldata tail)
    {
        head = x;
        tail = x[x.length:];
        if (x.length > 8) {
            assembly {
                tail.offset := add(x.offset, 8)
                tail.length := sub(x.length, 8)
            }
        }
    }
}
