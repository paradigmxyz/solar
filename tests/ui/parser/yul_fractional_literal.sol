// Yul has no fractions; see PARSE-005.
contract C {
    function f() public pure {
        assembly {
            switch 1
            case 0.5 {} //~ ERROR: invalid number literal
            default {}
            if 0.5 {} //~ ERROR: invalid number literal
            let x := 0.5 //~ ERROR: invalid number literal
        }
    }
}
