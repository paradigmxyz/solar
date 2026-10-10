//@ codegen-matrix: standard
//@ run-call: call => 2, 1
//@ run-call: libraryCall => 6, 1
//@ run-call: moduleConstant => 5, 1
//@ run-call: libraryConstant => 11, 1
//@ run-call: nestedModuleConstant => 13, 1
//@ run-call: structConstructor => 9, 1
//@ run-call: conversion => 0x0000000000000000000000000000000000001234, 1
//@ run-call: enumValue => 2, 1
//@ run-call: wrap => 3, 1
//@ run-call: functionValue => 5, 1
//@ run-call: functionSelector => 0xe2179b8e, 1
//@ run-call: errorSelector => 0x002ff067, 1
//@ run-call: eventSelector => 0xb30c6e011c2816674a43c08a53579e48c30e2158e65c09ae36edd17363863f98, 1
//@ run-call: statements => 5
//@ run-call: nested => 3
//@ run-call-fail: requireError => 0x002ff0670000000000000000000000000000000000000000000000000000000000000004
//@ run-call-fail: requireConstantMessage => Error("short")

import "./auxiliary/module_ternary_receiver_effects.sol" as M;

contract C {
    uint256 n;

    function bump() internal returns (bool) {
        n++;
        return true;
    }

    function call() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).f(1);
        return (r, n);
    }

    function libraryCall() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).L.h(3);
        return (r, n);
    }

    function moduleConstant() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).K;
        return (r, n);
    }

    function libraryConstant() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).L.LK;
        return (r, n);
    }

    function nestedModuleConstant() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).N.NK;
        return (r, n);
    }

    function structConstructor() public returns (uint256, uint256) {
        uint256 r = (bump() ? M : M).S(9).a;
        return (r, n);
    }

    function conversion() public returns (address, uint256) {
        address r = address((bump() ? M : M).D(address(0x1234)));
        return (r, n);
    }

    function enumValue() public returns (uint256, uint256) {
        uint256 r = uint256((bump() ? M : M).En.C);
        return (r, n);
    }

    function wrap() public returns (uint256, uint256) {
        uint256 r = M.U.unwrap((bump() ? M : M).U.wrap(3));
        return (r, n);
    }

    function functionValue() public returns (uint256, uint256) {
        function(uint256) pure returns (uint256) p = (bump() ? M : M).f;
        return (p(4), n);
    }

    function functionSelector() public returns (bytes4, uint256) {
        bytes4 r = (bump() ? M : M).D.g.selector;
        return (r, n);
    }

    function errorSelector() public returns (bytes4, uint256) {
        bytes4 r = (bump() ? M : M).E.selector;
        return (r, n);
    }

    function eventSelector() public returns (bytes32, uint256) {
        bytes32 r = (bump() ? M : M).Ev.selector;
        return (r, n);
    }

    function statements() public returns (uint256) {
        (bump() ? M : M);
        (bump() ? M : M).D;
        (bump() ? M : M).f;
        (bump() ? M : M).K;
        (bump() ? M : M).En.A;
        return n;
    }

    function nested() public returns (uint256) {
        ((bump() ? M : M).L);
        (bump() ? (bump() ? M : M) : M).K;
        return n;
    }

    function requireError() public {
        require(false, (bump() ? M : M).E(4));
    }

    function requireConstantMessage() public {
        require(false, (bump() ? M : M).MSG);
    }
}
