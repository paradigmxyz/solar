//@compile-flags: -Ztypeck

contract C {
    function f() public returns (uint256 x) {
        assembly {
            function one(a, b) -> r {
                r := add(a, b)
            }

            x := one(1) //~ ERROR: wrong argument count
        }
    }

    function h() public returns (uint256 x, uint256 y) {
        assembly {
            function pair(a) -> b, c {
                b := add(a, 1)
                c := add(a, 2)
            }

            x, y := pair(1)
        }
    }

    function nested() public returns (uint256 x) {
        assembly {
            function outer(a) -> r {
                function inner(b) -> c {
                    c := outer(b)
                }

                r := a
            }

            x := outer(1)
        }
    }
}
