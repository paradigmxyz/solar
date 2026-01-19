// Test Yul function definitions

contract YulFunctions {
    function singleReturn() public pure returns (uint256 result) {
        assembly {
            function double(x) -> y {
                y := mul(x, 2)
            }
            result := double(21)
        }
    }

    function multipleReturns() public pure returns (uint256 a, uint256 b) {
        assembly {
            function swap(x, y) -> r1, r2 {
                r1 := y
                r2 := x
            }
            a, b := swap(1, 2)
        }
    }

    function recursiveFunction() public pure returns (uint256 result) {
        assembly {
            function factorial(n) -> r {
                switch n
                case 0 { r := 1 }
                default { r := mul(n, factorial(sub(n, 1))) }
            }
            result := factorial(5)
        }
    }

    function earlyLeave() public pure returns (uint256 result) {
        assembly {
            function findFirst(target) -> idx {
                for { let i := 0 } lt(i, 100) { i := add(i, 1) } {
                    if eq(i, target) {
                        idx := i
                        leave
                    }
                }
                idx := 999
            }
            result := findFirst(42)
        }
    }

    function nestedFunctions() public pure returns (uint256 result) {
        assembly {
            function outer(x) -> r {
                function inner(y) -> z {
                    z := add(y, 1)
                }
                r := inner(inner(x))
            }
            result := outer(10)
        }
    }
}
