// Doc comments and unbalanced markers in sibling comments; isolates are not checked, like solc.

/// ‮doc
//~^ ERROR: mismatching directional override markers in comment
/** ‬ */
//~^ ERROR: unicode direction override underflow in comment
contract C {
    /// ok ‮‬
    function f() public pure returns (uint256 d, uint256 m) {
        (/*month‮*/, d, /*‬‬hour*/, m) = (1, 2, 3, 4);
        //~^ ERROR: mismatching directional override markers in comment
        //~| ERROR: unicode direction override underflow in comment
    }

    function g() public pure {
        // isolates ⁦ ⁩⁩
        string memory s = unicode"isolates ⁦";
        s = unicode"escapes \u202E";
        s = unicode"‬";
        //~^ ERROR: unicode direction override underflow in string literal
        s = unicode"‮";
        //~^ ERROR: mismatching directional override markers in string literal
        s;
    }
}
