//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: dynamic(bytes) 0x112233 => 3, 0x11, 7, 3, 0x11
//@ run-call: specialized(bytes) 0xaabbccdd => 4, 0xaa, 7, 4, 0xaa

contract InternalFunctionPointerCalldataReturns {
    function(bytes calldata)
        internal
        returns (bytes calldata, uint256, bytes calldata) target;

    constructor() {
        target = identity;
    }

    function dynamic(bytes calldata value)
        external
        returns (uint256, bytes1, uint256, uint256, bytes1)
    {
        (bytes calldata left, uint256 tag, bytes calldata right) = target(value);
        return (left.length, left[0], tag, right.length, right[0]);
    }

    function specialized(bytes calldata value)
        external
        pure
        returns (uint256, bytes1, uint256, uint256, bytes1)
    {
        function(bytes calldata)
            internal
            pure
            returns (bytes calldata, uint256, bytes calldata) selected = identity;
        (bytes calldata left, uint256 tag, bytes calldata right) = selected(value);
        return (left.length, left[0], tag, right.length, right[0]);
    }

    function identity(bytes calldata value)
        internal
        pure
        returns (bytes calldata, uint256, bytes calldata)
    {
        return (value, 7, value);
    }
}
