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
            //~^ ERROR: mismatched number of components
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
            side_effect_only(x) //~ ERROR: unresolved symbol
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

    function k() public returns (uint256 x) {
        assembly {
            let a := 1

            {
                function inner() -> r {
                    r := a //~ ERROR: unresolved symbol
                }

                x := inner()
            }
        }
    }

    function l() public returns (uint256 x) {
        assembly {
            let a := 1

            {
                let a := 2
                x := a
            }
        }
    }

    function m() public returns (uint256 x) {
        assembly {
            function y() -> r {
                r := 1
            }

            {
                let y := 2
                x := y
            }
        }
    }

    function n() public returns (uint256 x) {
        assembly {
            let y := 1

            {
                function y() -> r {
                    r := 2
                }

                x := y()
            }
        }
    }

    function o() public returns (uint256 x) {
        assembly {
            function y() -> r {
                r := 1
            }

            {
                function y() -> r {
                    r := 2
                }

                x := y()
            }
        }
    }
}
