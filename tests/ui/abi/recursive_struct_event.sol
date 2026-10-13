//@ compile-flags: --emit=abi
//@ filecheck:
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
    // CHECK: "name":"E1","inputs":[{"name":"","type":"tuple","indexed":false,"internalType":"struct S"}]
    event E1(S); //~ ERROR: recursive types cannot be event parameter types
    // CHECK: "name":"E2","inputs":[{"name":"","type":"tuple[2]","indexed":false,"internalType":"struct S[2]"}]
    event E2(S[2]); //~ ERROR: recursive types cannot be event parameter types
    // CHECK: "name":"E3","inputs":[{"name":"","type":"tuple","indexed":false,"internalType":"struct A"}]
    event E3(A); //~ ERROR: recursive types cannot be event parameter types
}
