pragma solidity ^0.8.20;
// Overloads of one bound name on two user-defined value types over the same underlying type:
// the receiver's declared type picks the overload, even when the receiver is a plain variable.
type A is uint256;
type B is uint256;

library Tag {
    function tag(A) internal pure returns (uint256) {
        return 1;
    }

    function tag(B) internal pure returns (uint256) {
        return 2;
    }
}

contract UdvtOverload {
    using Tag for A;
    using Tag for B;

    /// @custom:ensures r == 2
    function bad_var(uint256 x) external pure returns (uint256 r) {
        A a = A.wrap(x);
        r = a.tag();
    }

    /// @custom:ensures r == 1
    function ok_var(uint256 x) external pure returns (uint256 r) {
        A a = A.wrap(x);
        r = a.tag();
    }

    /// @custom:ensures r == 2
    function bad_param(A a) external pure returns (uint256 r) {
        r = a.tag();
    }

    /// @custom:ensures r == 1
    function ok_wrap(uint256 x) external pure returns (uint256 r) {
        r = A.wrap(x).tag();
    }
}
