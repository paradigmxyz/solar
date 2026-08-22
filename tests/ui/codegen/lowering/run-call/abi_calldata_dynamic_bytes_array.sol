//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: inspect [0x0102, 0x040506] => 2, 2, 1, 2, 3, 4, 5, 6
//@[none, gas, size] run-call: encode [0x0102, 0x040506] => 0xedff71de02e6a78c4e1b28325ab806b0c18e590be1ea92ca617d246559c679c4
//@[none, gas, size] run-call-fail: 0x04f02d88000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020
//@[none, gas, size] run-call-fail: 0x04f02d880000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_indexing_dynamic_bytes_v2.sol

pragma abicoder v2;

contract AbiCalldataDynamicBytesArray {
    function encode(bytes[] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encode(values));
    }

    function inspect(bytes[] calldata values)
        external
        pure
        returns (
            uint256 count,
            uint256 firstLength,
            uint256 firstByte,
            uint256 secondByte,
            uint256 secondLength,
            uint256 thirdByte,
            uint256 fourthByte,
            uint256 fifthByte
        )
    {
        count = values.length;
        firstLength = values[0].length;
        firstByte = uint8(values[0][0]);
        secondByte = uint8(values[0][1]);
        secondLength = values[1].length;
        thirdByte = uint8(values[1][0]);
        fourthByte = uint8(values[1][1]);
        fifthByte = uint8(values[1][2]);
    }
}
