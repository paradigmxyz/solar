//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    uint brokenState = * 2; //~[strict,recover] ERROR: expected one of

    function brokenStatement() internal {
        uint local = * 2; //~[recover] ERROR: expected one of
    }

    function later() internal pure {
        uint8 value = 300; //~[recover] ERROR: mismatched types
    }
}
