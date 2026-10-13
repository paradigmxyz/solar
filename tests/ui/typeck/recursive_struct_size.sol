// Size checks stop at recursive structs instead of recursing until the stack overflows.
contract C {
    struct S { //~ ERROR: recursive struct definition
        S s;
    }

    // solc also rejects `A` as a recursive struct definition, but we find the dynamic cycle
    // through `xs` first and classify it as recursive only.
    struct A {
        A[] xs;
        B b;
    }
    struct B {
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
