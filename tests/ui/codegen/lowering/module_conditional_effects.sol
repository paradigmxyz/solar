//@ compile-flags: -Zdump=mir

import "./auxiliary/module_conditional_effects.sol" as M;

type U is uint256;

using {eq as ==} for U global;

function eq(U a, U b) pure returns (bool) {
    return U.unwrap(a) == U.unwrap(b);
}

contract C is M.B {
    bool constant COMPUTED = 1 < 2;
    uint256 n;
    M.S s;

    function bump() internal returns (bool) {
        n++;
        return true;
    }

    function guard(uint256 x) internal pure returns (bool) {
        require(x != 0);
        return true;
    }

    function one() internal pure returns (uint256) {
        return 1;
    }

    function assignment() public pure returns (bool) {
        bool flag;
        ((flag = true) ? M : M).D; //~ ERROR: module conditionals whose condition may have side effects are not supported
        return flag;
    }

    function structStore() public {
        s = (bump() ? M : M).S(9); //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function tryLibrary() public returns (uint256) {
        try (bump() ? M : M).L.ext(1) returns (uint256 r) { //~ ERROR: module conditionals whose condition may have side effects are not supported
            return r;
        } catch {
            return 0;
        }
    }

    function encodeCall() public returns (bytes memory) {
        return abi.encodeCall((bump() ? M : M).D.g, ()); //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function decodeType() public returns (uint256) {
        return abi.decode(abi.encode(uint256(5)), ((bump() ? M : M).S)).a; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function libraryAddress() public returns (address) {
        return address((bump() ? M : M).L); //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function pureStruct(uint256 x) public pure returns (uint256) {
        return (guard(x) ? M : M).S(x).a; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function storageWrite(address who) public {
        (bump() ? M : M).B.owner = who; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function pointerCall() public returns (uint256) {
        fp = one;
        return (bump() ? M : M).B.fp(); //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function userOperator(U a) public pure returns (uint256) {
        return (a == U.wrap(0) ? M : M).K; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function computedConstant() public pure returns (uint256) {
        return (COMPUTED ? M : M).K; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function arithmetic(uint256 x) public pure returns (uint256) {
        return (1 / x > 0 ? M : M).K; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }

    function nested(bool c) public returns (uint256) {
        return (c ? (bump() ? M : M) : M).K; //~ ERROR: module conditionals whose condition may have side effects are not supported
    }
}
