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

    event E1(E2); //~ ERROR: name has to refer to a valid user-defined type
    event E2(E1); //~ ERROR: name has to refer to a valid user-defined type

    type U1 is U1; //~ ERROR: the underlying type of UDVTs must be an elementary value type

    function a(A memory) public {}
    function b(B memory) public {}
    function c(C memory) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions
    function d(E1 memory) public {} //~ ERROR: name has to refer to a valid user-defined type
    function e(E2 memory) public {} //~ ERROR: name has to refer to a valid user-defined type
    function f(U1 memory) public {}
}
