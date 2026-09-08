//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@[mir] filecheck: --check-prefix=SEMANTIC
//@ run-call: encode [] => 0x
//@ run-call: encode [0x303132333435363738393031323334353637383961626364] => 0x3031323334353637383930313233343536373839616263640000000000000000
//@ run-call-fail: 0xea460cbb000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000013031323334353637383930313233343536373839616263645800000000000000
//@ run-call: memoryEncode => 0x0000000000000000000000000000000000000000d4b839920000000000000000

contract AbiPackedFunctionPointerArray {
    function target() external pure {}

    function memoryEncode() external pure returns (bytes memory) {
        function() external[] memory pointers = new function() external[](1);
        pointers[0] = AbiPackedFunctionPointerArray(address(0)).target;
        return abi.encodePacked(pointers);
    }

    // SEMANTIC-LABEL: fn @encode(
    // SEMANTIC: panic_if {{v[0-9]+}}, 0x41
    // SEMANTIC: alloc memoryarray<1>
    // SEMANTIC: [[OFFSET:v[0-9]+]] = mul {{v[0-9]+}}, 32
    // SEMANTIC-NEXT: [[HEAD:v[0-9]+]] = add {{v[0-9]+}}, [[OFFSET]]
    // SEMANTIC-NEXT: [[VALUE:v[0-9]+]] = calldataload [[HEAD]]
    // SEMANTIC: and [[VALUE]], 0xffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000
    // SEMANTIC: revert_if {{v[0-9]+}}, empty
    function encode(function() external returns (uint256)[] calldata pointers)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(pointers);
    }
}
