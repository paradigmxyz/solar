// Size checks stop at recursive structs instead of recursing until the stack overflows.
contract C {
    struct S { //~ ERROR: recursive struct definition
        S s;
    }

    struct A { //~ ERROR: recursive struct definition
        A[] xs;
        B b;
    }
    struct B { //~ ERROR: recursive struct definition
        A a;
    }

    function direct(S[2] calldata s) external pure {
        s;
    }

    function mixed(A memory a) internal pure {
        a;
    }

    function mixedArray(A[2] memory a) internal pure {
        a;
    }
}
