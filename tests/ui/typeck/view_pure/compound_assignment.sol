//@compile-flags: -Ztypeck
// Tests that compound assignment on state variables is state-modifying.
// TODO: When view/pure checking is implemented, all of these should error with:
// function cannot be declared as view because this expression modifies the state

contract C {
    uint x;

    function f() view public {
        x += 1;
    }
    function g() view public {
        x -= 1;
    }
    function h() view public {
        x *= 2;
    }
    function i() view public {
        x /= 2;
    }
}
