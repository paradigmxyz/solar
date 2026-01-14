//@compile-flags: -Ztypeck
// Tests that delete on state variables in view/pure functions is an error.
// TODO: When view/pure checking is implemented, all of these should error with:
// function cannot be declared as view because this expression modifies the state

contract C {
    uint x;
    uint[] arr;
    mapping(uint => uint) m;

    function f() view public {
        delete x;
    }
    function g() view public {
        delete arr[0];
    }
    function h() view public {
        delete m[0];
    }
}
