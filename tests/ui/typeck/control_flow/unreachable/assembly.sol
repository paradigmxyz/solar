// The blocks of inline assembly `for` loops add only their statements to the range.
contract C {
    function revertingPre() public pure {
        assembly {
            for { revert(0, 0) } 1 { mstore(0, 2) } { function q() {} mstore(0, 1) }
            //~^ WARN: unreachable code
            //~| WARN: unreachable code
        }
    }

    function nestedBody() public pure {
        assembly {
            for { return(0, 0) } 1 {} { { mstore(0, 1) } } //~ WARN: unreachable code
        }
    }

    function afterFor() public pure {
        assembly {
            for { let i := 0 } lt(i, 2) { i := add(i, 1) } { if i { break } mstore(0, i) }
            return(0, 0)
            for { } 1 { } { mstore(0, 1) } //~ WARN: unreachable code
        }
    }

    function revertingPost() public pure {
        assembly {
            for { let i := 0 } lt(i, 2) { revert(0, 0) } { mstore(0, i) }
            mstore(0, 3)
        }
    }

    function nestedFor() public pure {
        assembly {
            for { let i := 0 } lt(i, 2) { i := add(i, 1) } { for { revert(0, 0) } 1 {} { mstore(0, i) } }
            //~^ WARN: unreachable code
            //~| WARN: unreachable code
        }
    }
}
