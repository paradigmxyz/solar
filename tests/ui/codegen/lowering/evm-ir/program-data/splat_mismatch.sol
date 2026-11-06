//@ compile-flags: -Osize
//@ run-call: dataHash => 0xe46e89901e32441fa888385d27759897c8b836b525f4e0ae3133748d94cccbf8

// Only the middle word differs from the first word. The data is not one
// repeated word, so it must not be built by doubling it with `mcopy`.
contract C {
    function data() public pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100112233445566778899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677";
    }

    function dataHash() external pure returns (bytes32) {
        return keccak256(data());
    }
}
