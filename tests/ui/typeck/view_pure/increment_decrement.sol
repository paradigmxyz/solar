//@compile-flags: -Ztypeck
// Tests that increment/decrement on state variables is state-modifying.
// TODO: When view/pure checking is implemented, all of these should error with:
// function cannot be declared as view because this expression modifies the state

contract C {
    uint x;

    function f() view public {
        x++;
    }
    function g() view public {
        x--;
    }
    function h() view public {
        ++x;
    }
    function i() view public {
        --x;
    }
}
