//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard
//@[size] run-call: raw() => 0x0102030405

contract MCopySharedHelperRaw {
    function join(bytes memory a, bytes memory b) external pure returns (bytes memory) {
        return abi.encodePacked(a, b);
    }

    function twice(bytes memory a) external pure returns (bytes memory) {
        return bytes.concat(a, a);
    }

    // The pointer is not backed by a compiler allocation. A shared copy helper
    // must not stage its arguments over the source object at 0xa0.
    function raw() external pure returns (bytes memory) {
        bytes memory value;
        assembly {
            value := 0xa0
            mstore(value, 3)
            mstore(add(value, 32), shl(232, 0x010203))
        }
        return bytes.concat(value, hex"0405");
    }
}
