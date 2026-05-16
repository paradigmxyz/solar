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
}
