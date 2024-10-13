contract CC {
    // TODO: Reject these 2
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

    // TODO: cannot print signature recursively
    // function f(A, B, C, E1, E2, U1) public {}
}
