contract CC {
    struct A { //~ ERROR: recursive struct definition
        B b;
    }
    struct B { //~ ERROR: recursive struct definition
        A a;
    }

    struct C {
        C[] c;
    }

    function a1(A memory) public {}
    function b1(B memory) public {}
    function c1(C memory) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions

    function a2() public returns(A memory) {}
    function b2() public returns(B memory) {}
    function c2() public returns(C memory) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
}
