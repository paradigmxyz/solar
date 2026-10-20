//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: three 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: empty() => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: addresses 0x0000000000000000000000000000000000001234 => 0xe321d900f3fd366734e2d071e30949ded20c27fd638f1a059390091c643b62c5

// The packed encoding of a dynamic memory array of full-word value types is its
// elements as ABI words, which is how memory holds them, so a hash of it reads
// the array's elements in place instead of a copy. The encoding cleans narrower
// elements, so an array of addresses still takes the encoder.
// CHECK-LABEL: fn @three(
// CHECK: mstore [[DATA:v[0-9]+]], arg0
// CHECK: keccak256 [[DATA]], 96
contract KeccakPackedWordArray {
    function three(uint256 x, uint256 y, uint256 z) external pure returns (bytes32) {
        uint256[] memory a = new uint256[](3);
        a[0] = x;
        a[1] = y;
        a[2] = z;
        return keccak256(abi.encodePacked(a));
    }

    function empty() external pure returns (bytes32) {
        bytes32[] memory a;
        return keccak256(abi.encodePacked(a));
    }

    function addresses(address x) external pure returns (bytes32) {
        address[] memory a = new address[](1);
        a[0] = x;
        return keccak256(abi.encodePacked(a));
    }
}
