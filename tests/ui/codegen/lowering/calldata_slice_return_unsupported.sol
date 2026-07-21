//@compile-flags: -Zcodegen --emit=mir
//@check-fail

contract CalldataSliceReturnUnsupported {
    // A calldata slice returned from an internal function crosses the call
    // boundary as an `(offset, length)` pair, which slice lowering does not
    // expand on the return side. A straight-line helper is inlined, but one
    // with statement-level control flow cannot be, so it is reported rather
    // than lowered to a slice the backend cannot handle.
    function decode(bytes calldata data) //~ ERROR: returning a `bytes`/`string` calldata slice from this internal function is not yet supported in codegen
        internal
        pure
        returns (bytes calldata head)
    {
        head = data;
        if (data.length > 32) {
            assembly {
                head.length := 32
            }
        }
    }

    function use(bytes calldata data) external pure {
        decode(data);
    }
}
