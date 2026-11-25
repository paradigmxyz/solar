pragma solidity ^0.8.20;
struct S { uint256[1] arr; uint256 y; }
contract FirstArg {
    function _flip(uint256[1] memory a) private pure returns (uint256) {
        a[0] = ~a[0];
        return 2;
    }
    function _id(uint256[1] memory a, uint256) private pure returns (uint256[1] memory) {
        return a;
    }
    /// @custom:ensures result
    function bad_struct_ctor(uint256 x) external pure returns (bool) {
        uint256[1] memory a;
        a[0] = x;
        bytes32 h1 = keccak256(abi.encode(S(a, 2)));
        bytes32 h2 = keccak256(abi.encode(S(a, _flip(a))));
        return h1 == h2;
    }
    /// @custom:ensures result
    function bad_id_call(uint256 x) external pure returns (bool) {
        uint256[1] memory a;
        a[0] = x;
        bytes32 h1 = keccak256(abi.encode(a));
        bytes32 h2 = keccak256(abi.encode(_id(a, _flip(a))));
        return h1 == h2;
    }
    /// @custom:ensures result
    function ok_struct_ctor(uint256 x) external pure returns (bool) {
        uint256[1] memory a;
        a[0] = x;
        bytes32 h1 = keccak256(abi.encode(S(a, 2)));
        bytes32 h2 = keccak256(abi.encode(S(a, 2)));
        return h1 == h2;
    }
}
