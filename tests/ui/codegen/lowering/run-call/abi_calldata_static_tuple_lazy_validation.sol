//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: 0x4637216b0000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0xb7a18a020000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0x5d8a477900000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002 => 0x0000000000000000000000000000000000000000000000000000000000000040

contract AbiCalldataStaticTupleLazyValidation {
    struct Values {
        uint8 first;
        uint8 second;
    }

    struct Words {
        uint256 first;
        uint256 second;
    }

    function use(Values calldata values) external pure returns (uint256) {
        return values.first;
    }

    function unused(Values calldata) external pure returns (uint256) {
        return 1;
    }

    function encodeWords(Words calldata values) external pure returns (uint256) {
        return abi.encode(values).length;
    }
}
