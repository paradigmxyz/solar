//@ codegen-matrix: standard
//@ run-call: call true => 2
//@ run-call: libraryCall false => 6
//@ run-call: moduleConstant => 5
//@ run-call: libraryConstant 3 => 11
//@ run-call: nestedModuleConstant true, 0 => 13
//@ run-call: structConstructor => 9
//@ run-call: conversion false => 0x0000000000000000000000000000000000001234
//@ run-call: enumValue => 2
//@ run-call: wrap true => 3
//@ run-call: functionValue true => 5
//@ run-call: functionSelector true => 0xe2179b8e
//@ run-call: errorSelector true => 0x002ff067
//@ run-call: eventSelector true => 0xb30c6e011c2816674a43c08a53579e48c30e2158e65c09ae36edd17363863f98
//@ run-call: statements true => 1
//@ run-call: nested true, false => 13
//@ run-call-fail: requireError true => 0x002ff0670000000000000000000000000000000000000000000000000000000000000004
//@ run-call-fail: requireConstantMessage true => Error("short")

import "./auxiliary/module_conditional_members.sol" as M;

contract C {
    bool constant LITERAL = true;
    bool flag;

    function call(bool c) public pure returns (uint256) {
        return (c ? M : M).f(1);
    }

    function libraryCall(bool c) public pure returns (uint256) {
        return (!c ? M : M).L.h(3);
    }

    function moduleConstant() public pure returns (uint256) {
        return (true ? M : M).K;
    }

    function libraryConstant(uint256 x) public pure returns (uint256) {
        return (x > 2 ? M : M).L.LK;
    }

    function nestedModuleConstant(bool c, uint256 x) public pure returns (uint256) {
        return (c && x == 0 ? M : M).N.NK;
    }

    function structConstructor() public view returns (uint256) {
        return (flag ? M : M).S(9).a;
    }

    function conversion(bool c) public pure returns (address) {
        return address((c || false ? M : M).D(address(0x1234)));
    }

    function enumValue() public pure returns (uint256) {
        return uint256((LITERAL ? M : M).En.C);
    }

    function wrap(bool c) public pure returns (uint256) {
        return M.U.unwrap((c ? M : M).U.wrap(3));
    }

    function functionValue(bool c) public pure returns (uint256) {
        function(uint256) pure returns (uint256) p = (c ? M : M).f;
        return p(4);
    }

    function functionSelector(bool c) public pure returns (bytes4) {
        return (c ? M : M).D.g.selector;
    }

    function errorSelector(bool c) public pure returns (bytes4) {
        return (c ? M : M).E.selector;
    }

    function eventSelector(bool c) public pure returns (bytes32) {
        return (c ? M : M).Ev.selector;
    }

    function statements(bool c) public pure returns (uint256) {
        (c ? M : M);
        (c ? M : M).D;
        (c ? M : M).f;
        (c ? M : M).K;
        (c ? M : M).En.A;
        return 1;
    }

    function nested(bool c, bool d) public pure returns (uint256) {
        ((c ? M : M).L);
        return (c ? (d ? M : M) : M).N.NK;
    }

    function requireError(bool c) public pure {
        require(false, (c ? M : M).E(4));
    }

    function requireConstantMessage(bool c) public pure {
        require(false, (c ? M : M).MSG);
    }
}
