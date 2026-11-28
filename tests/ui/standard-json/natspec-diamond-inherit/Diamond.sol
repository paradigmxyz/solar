// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.9.0;

interface I {
    /// @notice From I.
    function f() external;
}

abstract contract G is I {
    /// @inheritdoc I
    function f() public virtual {}

    /// @notice From G.
    function g() public virtual {}
}

abstract contract S is G {}

abstract contract Q is G {}

abstract contract Diamond is S, Q {
    function f() public override {}

    function g() public override {}
}

abstract contract A {
    /// @notice From A.
    function h() public virtual {}
}

abstract contract B {
    /// @notice From B.
    function h() public virtual {}
}

abstract contract Distinct is A, B {
    function h() public override(A, B) {}
}
