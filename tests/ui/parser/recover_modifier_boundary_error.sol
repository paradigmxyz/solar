//@ revisions: strict recover
//@ compile-flags: --stop-after parsing
//@[recover] compile-flags: -Zrecover-incomplete-input

// Modifier arguments share call parsing but must still reject a statement in the header.
contract C {
    modifier m(uint256 x) { _; }

    function f() m(1 +
        if (true) {}) {} //~ ERROR: expected
        //~[recover]^ ERROR: expected
}
