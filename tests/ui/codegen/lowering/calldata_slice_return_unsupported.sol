//@compile-flags: -Zcodegen --emit=bin-runtime

contract CalldataSliceReturnUnsupported {
    // A calldata slice returned from an internal function is inlined at the
    // call site so it folds away — straight-line bodies, control flow, explicit
    // returns, and multiple returns all inline. Recursion is the shape that
    // cannot: inlining would not terminate, and a real `internal_call` would
    // hand back a slice that is materialized at the external boundary.
    function peel(bytes calldata data)
        internal
        pure
        returns (bytes calldata)
    {
        if (data.length < 2) return data;
        return peel(data[1:]);
    }

    function use(bytes calldata data) external pure {
        peel(data);
    }
}
