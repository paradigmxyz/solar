//@compile-flags: -Zcodegen --emit=bin-runtime
//@check-fail

contract CalldataSliceReturnUnsupported {
    // A calldata slice returned from an internal function is inlined at the call
    // site so it folds away: a straight-line body, or a control-flow body whose
    // returns are implicit named ones, both inline. A control-flow body with an
    // explicit `return` cannot — full block lowering would turn the `return`
    // into a terminator that returns from the caller — so it is reported rather
    // than lowered to a slice the backend cannot handle.
    function decode(bytes calldata data) //~ ERROR: returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen
        internal
        pure
        returns (bytes calldata)
    {
        if (data.length > 32) return data[32:];
        return data;
    }

    function use(bytes calldata data) external pure {
        decode(data);
    }
}
