//@compile-flags: -Ztypeck
// Tests that reading block properties in view functions is allowed.

contract C {
    function f() view public returns (uint) {
        return block.timestamp;
    }
    function g() view public returns (uint) {
        return block.number;
    }
    function h() view public returns (address) {
        return block.coinbase;
    }
    function i() view public returns (uint) {
        return block.prevrandao;
    }
    function j() view public returns (uint) {
        return block.chainid;
    }
    function k() view public returns (uint) {
        return block.basefee;
    }
    function l() view public returns (uint) {
        return block.gaslimit;
    }
}
