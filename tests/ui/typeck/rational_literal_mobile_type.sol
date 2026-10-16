// Fractional literals have no mobile type, so expressions that need one reject them; see PARSE-004.
contract C {
    function f() public pure {
        0.3 < 0.5; //~ ERROR: cannot apply builtin operator `<`
        true ? 0.5 : 1.5; //~ ERROR: invalid true type
        //~^ ERROR: invalid false type
        [0.5, 1.5]; //~ ERROR: invalid mobile type
    }
}
