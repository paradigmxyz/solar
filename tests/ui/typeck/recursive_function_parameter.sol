contract CC {
    struct A {
        B b;
    }
    struct B {
        A a;
    }

    struct C {
        C[] c;
    }

    function a1(A) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function b1(B) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function c1(C) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions

    function a2() public returns(A) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function b2() public returns(B) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function c2() public returns(C) {} //~ ERROR: recursive types cannot be parameter or return types of public functions
}
