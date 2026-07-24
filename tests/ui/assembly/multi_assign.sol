// Test multi-variable declarations and assignments

contract MultiAssign {
    function multiDeclareNoInit() public pure returns (uint256 a, uint256 b) {
        assembly {
            let x, y
            x := 1
            y := 2
            a := x
            b := y
        }
    }

    function multiDeclareWithCall() public pure returns (uint256 a, uint256 b) {
        assembly {
            function getPair() -> x, y {
                x := 10
                y := 20
            }
            let r1, r2 := getPair()
            a := r1
            b := r2
        }
    }

    function multiAssign() public pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            function getTriple() -> x, y, z {
                x := 1
                y := 2
                z := 3
            }
            a, b, c := getTriple()
        }
    }

    function partialAssign() public pure returns (uint256 result) {
        assembly {
            function getTwoValues() -> x, y {
                x := 100
                y := 200
            }
            let a, b := getTwoValues()
            result := add(a, b)
        }
    }
}
