// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
// A base's spec names `msg.sender`; the derived contract alone declares a variable `msg`, so
// the inherited spec would read its storage there instead.
contract Base {
    address owner;
    // expect: `msg` names no state variable where the spec is written (in `Base`) but a state variable in `Derived`
    /// @custom:ensures owner == msg.sender
    function f() external { owner = msg.sender; }
}
contract Derived is Base {
    struct S { address sender; }
    S msg;
}
