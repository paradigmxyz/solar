//@compile-flags: -Ztypeck
// Tests that writing state variables in pure functions is an error.
// TODO: When view/pure checking is implemented:
// function cannot be declared as pure because this expression modifies the state

contract C {
    uint x;
    function f() pure public {
        x = 1;
    }
}
