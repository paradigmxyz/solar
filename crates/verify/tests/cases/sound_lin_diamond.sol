// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract A {
    function f() public virtual returns (uint256) { return 1; }
}
contract B {
    function f() public virtual returns (uint256) { return 2; }
}
contract C is A, B {
    function f() public virtual override(A, B) returns (uint256) { return super.f(); }
}
// C3 linearization of D: D, C, B, A. super.f() in C (run for D) is B.f, so D.f() == 2.
contract D is B, C {
    function f() public override(B, C) returns (uint256) { return super.f(); }

    /// @custom:ensures r == 1
    function bad_lin() public returns (uint256 r) { r = f(); }

    /// @custom:ensures r == 2
    function ok_lin() public returns (uint256 r) { r = f(); }
}
