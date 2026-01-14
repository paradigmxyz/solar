//@compile-flags: -Ztypeck
// Tests that reading block properties in pure functions is an error.
// TODO: When view/pure checking is implemented, all of these should error with:
// function declared as pure, but this expression reads from the environment or state

contract C {
    function f() pure public returns (uint) {
        return block.timestamp;
    }
    function g() pure public returns (uint) {
        return block.number;
    }
    function h() pure public returns (address) {
        return block.coinbase;
    }
}
