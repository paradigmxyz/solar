//@compile-flags: -Ztypeck

contract C {
    function f() public returns (uint256 x, uint256 y) {
        assembly {
            function pair(a) -> b, c {
                b := add(a, 1)
                c := add(a, 2)
            }
        }

        assembly {
            x, y := pair(1) //~ ERROR: unresolved symbol
        }
    }

    function g() public returns (uint256 x) {
        assembly {
            {
                function nested(a) -> b {
                    b := add(a, 1)
                }
            }

            x := nested(1) //~ ERROR: unresolved symbol
        }
    }

    function h() public returns (uint256 x) {
        assembly {
            x := pair(1) //~ ERROR: unresolved symbol
        }
    }

    function i() public returns (uint256 x, uint256 y) {
        assembly {
            function outer(a) -> r {
                function inner(b) -> c {
                    c := add(b, 1)
                }

                r := inner(a)
            }

            x := outer(1)
            y := inner(1) //~ ERROR: unresolved symbol
        }
    }

    function j() public returns (uint256 x) {
        assembly {
            function outer(a) -> r {
                function inner(b) -> c {
                    c := r //~ ERROR: unresolved symbol
                }

                r := inner(a)
            }

            x := outer(1)
        }
    }
}
