pragma solidity ^0.8.20;
// In memory a `uint256[1]` field is a pointer, not the element it holds: assembly that reads
// the struct head must not be modelled as reading `a[0]`.
struct S { uint256[1] a; }
contract MemArrayField {
    /// @custom:ensures r == x
    function head(uint256 x) external pure returns (uint256 r) {
        S memory s;
        s.a[0] = x;
        assembly { r := mload(s) }
    }
}
