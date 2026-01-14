//@compile-flags: -Ztypeck
// Tests that reading state variables in pure functions is an error.
// TODO: When view/pure checking is implemented:
// function declared as pure, but this expression reads from the environment or state

contract C {
    uint x;
    function f() pure public returns (uint) {
        return x;
    }
}
