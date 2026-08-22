//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: 0xea460cbb000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000013031323334353637383930313233343536373839616263640000000000000000 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000203031323334353637383930313233343536373839616263640000000000000000
//@ run-call-fail: 0xea460cbb000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000013031323334353637383930313233343536373839616263645800000000000000
//@ run-call: memoryEncode() => 0x0000000000000000000000000000000000000000d4b839920000000000000000

contract AbiPackedFunctionPointerArray {
    function target() external pure {}

    function memoryEncode() external pure returns (bytes memory) {
        function() external[] memory pointers = new function() external[](1);
        pointers[0] = AbiPackedFunctionPointerArray(address(0)).target;
        return abi.encodePacked(pointers);
    }

    function encode(function() external returns (uint256)[] calldata pointers)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(pointers);
    }
}
