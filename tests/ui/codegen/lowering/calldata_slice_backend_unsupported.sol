//@compile-flags: -Zcodegen --emit=bin-runtime
//@check-fail

// Destructuring a function that returns *two* calldata slices needs the tail
// value to cross the call boundary through the one-word-per-value multi-return
// buffer, which cannot carry a two-word slice. The tail is a `make_slice` the
// backend cannot emit, so it is reported as an unsupported construct rather
// than reaching the word-based backend and panicking. (A discarded or
// single-slice return inlines and folds; only destructured multi-slice returns
// hit this.)
contract CalldataSliceBackendUnsupported { //~ ERROR: codegen does not support this calldata-slice usage yet
    function _empty() internal pure returns (bytes calldata d) {
        assembly {
            d.length := 0
        }
    }

    function decode(bytes calldata data)
        internal
        pure
        returns (bytes calldata head, bytes calldata tail)
    {
        head = data;
        tail = _empty();
        if (data.length > 32) {
            assembly {
                tail.offset := add(data.offset, 32)
                tail.length := sub(data.length, 32)
            }
        }
    }

    function use(bytes calldata data) external pure returns (uint256) {
        (bytes calldata head, bytes calldata tail) = decode(data);
        return head.length + tail.length;
    }
}
