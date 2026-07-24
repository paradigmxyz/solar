//@ check-pass
contract C {
    function f(uint256 n) public pure returns (uint256 x) {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                if iszero(eq(i, 3)) {
                    x := add(x, i)
                }
            }

            switch x
            case 0 {
                x := 11
            }
            case 1 {
                x := 22
            }
            default {
                x := 33
            }
        }
    }
}
