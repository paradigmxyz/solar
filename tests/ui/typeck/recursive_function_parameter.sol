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

    function a1(A) public {}
    function b1(B) public {}
    function c1(C) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions

    function a2() public returns(A) {}
    function b2() public returns(B) {}
    function c2() public returns(C) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
}
