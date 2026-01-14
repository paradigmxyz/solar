//@compile-flags: -Ztypeck
// Tests that reading tx properties in pure functions is an error.
// TODO: When view/pure checking is implemented, all of these should error with:
// function declared as pure, but this expression reads from the environment or state

contract C {
    function f() pure public returns (address) {
        return tx.origin;
    }
    function g() pure public returns (uint) {
        return tx.gasprice;
    }
}
