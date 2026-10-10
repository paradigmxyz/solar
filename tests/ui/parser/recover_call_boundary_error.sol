//@ revisions: strict recover
//@ compile-flags: --stop-after parsing
//@[recover] compile-flags: -Zrecover-incomplete-input

// A recovered argument error must let the enclosing condition reject the same token.
contract C {
    function f() internal {
        if (add(1 +
            if (true) {}) {} //~ ERROR: expected
            //~[recover]^ ERROR: expected
    }
}
