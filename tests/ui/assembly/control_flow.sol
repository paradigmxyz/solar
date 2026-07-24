// Test Yul control flow constructs

contract ControlFlow {
    function ifStatement(uint256 x) public pure returns (uint256 result) {
        assembly {
            if gt(x, 10) {
                result := 1
            }
        }
    }

    function forLoop() public pure returns (uint256 sum) {
        assembly {
            for { let i := 0 } lt(i, 10) { i := add(i, 1) } {
                sum := add(sum, i)
            }
        }
    }

    function switchStatement(uint256 sel) public pure returns (uint256 result) {
        assembly {
            switch sel
            case 0 { result := 100 }
            case 1 { result := 200 }
            default { result := 999 }
        }
    }

    function breakContinue() public pure returns (uint256 sum) {
        assembly {
            for { let i := 0 } lt(i, 20) { i := add(i, 1) } {
                if eq(i, 15) { break }
                if eq(mod(i, 2), 0) { continue }
                sum := add(sum, i)
            }
        }
    }

    function nestedLoops() public pure returns (uint256 count) {
        assembly {
            for { let i := 0 } lt(i, 5) { i := add(i, 1) } {
                for { let j := 0 } lt(j, 5) { j := add(j, 1) } {
                    count := add(count, 1)
                }
            }
        }
    }
}
