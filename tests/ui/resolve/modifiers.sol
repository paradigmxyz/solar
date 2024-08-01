contract C {
    uint y;

    modifier m1() { _; }
    modifier m2(uint x) { x; _; }
}

contract D is C {
    function f1_1() public pure m1 {}
    function f2_1() public pure m1() {}
    function g_1() public view m2(y) {}
}

contract E is D {
    function f1_2() public pure m1 {}
    function f2_2() public pure m1() {}
    function g_2() public view m2(y) {}
}

contract Bad is E {
    function bad1() public f1_2 {} //~ ERROR expected modifier, found function
    function bad2() public f1_2() {} //~ ERROR expected modifier, found function
}
