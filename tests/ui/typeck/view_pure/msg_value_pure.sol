//@compile-flags: -Ztypeck
// Tests that reading msg.value in pure functions is an error.
// TODO: When view/pure checking is implemented:
// function declared as pure, but this expression reads from the environment or state

contract C {
    function f() pure public returns (uint) {
        return msg.value;
    }
}
