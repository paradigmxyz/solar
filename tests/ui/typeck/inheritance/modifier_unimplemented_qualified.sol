abstract contract D {
    modifier m(uint) virtual;
    function f() D.m(1) public {} //~ ERROR: cannot call unimplemented modifier
    function g() m(1) public {}
}

contract E is D {
    modifier m(uint) override { _; }
    function h() D.m(1) public {} //~ ERROR: cannot call unimplemented modifier
    function i() E.m(1) public {}
    function j() m(1) public {}
}
