//@compile-flags: -Zcodegen --emit=bin-runtime

contract CalldataSliceReturnUnsupported {
    // A calldata slice returned from an internal function is inlined at the
    // call site so it folds away — straight-line bodies, control flow, explicit
    // returns, and multiple returns all inline. Recursion is the shape that
    // cannot: inlining would not terminate, and a real `internal_call` would
    // hand back a slice the word-based backend cannot lower, so it is reported.
    function peel(bytes calldata data) //~ ERROR: returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen
        //~^ ERROR: returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen
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
