//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CDSLICE

contract CalldataSliceYulBounds {
    // Assigning `.offset`/`.length` in assembly rewrites one component of a
    // `bytes calldata` slice's `(offset, length)` pair. Building an empty slice
    // (`data.length := 0`) and returning it from an internal helper is the
    // `bytes calldata` empty-calldata idiom; the helper inlines so the slice is
    // reconstructed at the call site and folds to its length.

    function _empty() internal pure returns (bytes calldata data) {
        assembly {
            data.length := 0
        }
    }

    // The helpers inline, so the empty slice is a `make_calldata_slice` at the
    // call site (later folded away) with no `internal_call` left behind.
    // CDSLICE-LABEL: fn @emptyLen{{[( ]}}
    // CDSLICE: make_calldata_slice 0, 0
    // CDSLICE-NOT: internal_call
    function emptyLen() external pure returns (uint256) {
        return _sink(_empty());
    }

    // CDSLICE-LABEL: fn @_sink{{[( ]}}
    function _sink(bytes calldata data) internal pure returns (uint256) {
        return data.length;
    }

    // Trimming a slice in place: read back the new length after adjusting both
    // components.
    // CDSLICE-LABEL: fn @trimLen{{[( ]}}
    // CDSLICE: slice_ptr
    // CDSLICE: add {{.*}}, 4
    // CDSLICE: slice_len
    // CDSLICE: sub {{.*}}, 4
    // CDSLICE: make_calldata_slice
    function trimLen(bytes calldata x) external pure returns (uint256) {
        bytes calldata y = x;
        assembly {
            y.offset := add(x.offset, 4)
            y.length := sub(x.length, 4)
        }
        return y.length;
    }

    // A slice reassigned only inside a branch must merge through its two-word
    // slot, so the untaken path keeps the original length. A bare SSA update
    // would leak the branch value; the slot store/load is the branch merge.
    // CDSLICE-LABEL: fn @conditional{{[( ]}}
    // CDSLICE: mstore
    // CDSLICE: mload
    function conditional(bytes calldata x) external pure returns (uint256) {
        bytes calldata y = x;
        if (x.length > 32) {
            assembly {
                y.length := 32
            }
        }
        return y.length;
    }
}
