// The inline assembly check for circular constants also applies to suffixed names.
contract C {
    uint constant A = B;
    uint constant B = A;

    function f() public pure {
        assembly {
            let x := A.slot //~ ERROR: constant variable is circular
            let y := B.offset //~ ERROR: constant variable is circular
        }
    }
}
