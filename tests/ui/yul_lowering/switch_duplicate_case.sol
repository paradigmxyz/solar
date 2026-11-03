// ported-from: test/libyul/yulSyntaxTests/switch_duplicate_case.yul
// ported-from: test/libyul/yulSyntaxTests/switch_duplicate_case_different_literal.yul
// ported-from: test/libyul/yulSyntaxTests/switch_statement_duplicate_case.yul

contract C {
    function sameNumber() public pure {
        assembly {
            switch 0
            case 0 {} //~ NOTE: first defined here
            case 0 {} //~ ERROR: duplicate case `0` defined
        }
    }

    function differentLiteral() public pure {
        assembly {
            switch 0
            case 0 {} //~ NOTE: first defined here
            case 0x0 {} //~ ERROR: duplicate case `0x0` defined
        }
    }

    function emptyStringAndZero() public pure {
        assembly {
            switch 0
            case 0 {} //~ NOTE: first defined here
            case "" {} //~ ERROR: duplicate case `""` defined
        }
    }

    function boolAndOne() public pure {
        assembly {
            switch 0
            case true {} //~ NOTE: first defined here
            case 1 {} //~ ERROR: duplicate case `1` defined
        }
    }

    function stringAndPaddedNumber() public pure {
        assembly {
            switch 0
            case "a" {} //~ NOTE: first defined here
            case 0x6100000000000000000000000000000000000000000000000000000000000000 {} //~ ERROR: duplicate case `0x6100000000000000000000000000000000000000000000000000000000000000` defined
        }
    }

    function thirdDuplicate() public pure {
        assembly {
            switch 0
            case 1 {} //~ NOTE: first defined here
            case 2 {} //~ NOTE: first defined here
            case 1 {} //~ ERROR: duplicate case `1` defined
            case 2 {} //~ ERROR: duplicate case `2` defined
            default {}
        }
    }

    function repeatedValue() public pure {
        assembly {
            switch 0
            case 7 {} //~ NOTE: first defined here
            case 7 {} //~ ERROR: duplicate case `7` defined
            case 7 {} //~ ERROR: duplicate case `7` defined
        }
    }

    function distinct() public pure {
        assembly {
            switch 0
            case 0 {}
            case 1 {}
            case "a" {}
            default {}
        }
    }
}
