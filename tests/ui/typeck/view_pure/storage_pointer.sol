//@compile-flags: -Ztypeck
// Tests storage pointer modification in view functions.
// TODO: When view/pure checking is implemented:
// function cannot be declared as view because this expression modifies the state

contract C {
    struct S { uint x; }
    S s;

    function f() view public {
        S storage ptr = s;
        ptr.x = 1;
    }
}
