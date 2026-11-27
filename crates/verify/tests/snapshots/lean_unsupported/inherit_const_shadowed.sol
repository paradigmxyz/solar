// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
uint256 constant K = 1;
// A base's spec names the file-level constant; the derived contract's own `K` shadows it there.
contract Base {
    uint256 n;
    // expect: `K` names a state variable where the spec is written (in `Base`) but another state variable in `Derived`
    /// @custom:ensures n == K
    function f() external { n = 1; }
}
contract Derived is Base {
    uint256 constant K = 2;
}
