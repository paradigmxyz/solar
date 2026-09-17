//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard
//@[size] run-call: raw() => 0x0102030405
//@[size] run-call: freshReturn() => 0x010203

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

    // A fresh-object summary does not prove where an internal callee placed
    // the object after resetting the free-memory pointer.
    function freshReturn() external pure returns (bytes memory) {
        return fresh();
    }

    function fresh() internal pure returns (bytes memory value) {
        assembly {
            mstore(0x40, 0x140)
        }
        value = new bytes(3);
        value[0] = 0x01;
        value[1] = 0x02;
        value[2] = 0x03;
    }
}
