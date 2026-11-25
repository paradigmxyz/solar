pragma solidity ^0.8.20;
// A write inside the one abi.encode argument (a struct constructor's later field) changes an
// array an earlier field refers to before the compiled code encodes the struct.
contract EncodeStructField {
    struct S {
        uint256[1] a;
        uint256 b;
    }

    // expect: postcondition not satisfied
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(1)))
    function bad_struct_field_write(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(S(a, a[0] = 1)));
    }

    /// @custom:ensures r == keccak256(abi.encode(x, uint256(1)))
    function ok_struct_no_write(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(S(a, 1)));
    }
}
