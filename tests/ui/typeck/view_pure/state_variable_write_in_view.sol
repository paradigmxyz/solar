//@compile-flags: -Ztypeck
// Tests that writing state variables in view functions is an error.
// TODO: When view/pure checking is implemented:
// function cannot be declared as view because this expression modifies the state

contract C {
    uint x;
    function f() view public {
        x = 1;
    }
}
