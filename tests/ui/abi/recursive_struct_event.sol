//@ compile-flags: --emit=abi
// Regression test for https://github.com/paradigmxyz/solar/issues/1841.

struct S {
    S[] s;
}

struct A {
    B[] b;
}

struct B {
    A[] a;
}

contract C {
    event E1(S); //~ ERROR: recursive types cannot be event parameter types
    event E2(S[2]); //~ ERROR: recursive types cannot be event parameter types
    event E3(A); //~ ERROR: recursive types cannot be event parameter types
}
