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

    event E1(E2); //~ ERROR: name has to refer to a valid user-defined type
    event E2(E1); //~ ERROR: name has to refer to a valid user-defined type

    type U1 is U1; //~ ERROR: the underlying type of UDVTs must be an elementary value type

    function a(A) public {}
    function b(B) public {}
    function c(C) public {}
    function d(E1) public {} //~ ERROR: name has to refer to a valid user-defined type
    function e(E2) public {} //~ ERROR: name has to refer to a valid user-defined type
    function f(U1) public {}
}
