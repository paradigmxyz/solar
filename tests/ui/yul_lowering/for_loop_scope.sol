//@ compile-flags: -Ztypeck

contract C {
    function init_visible(uint256 n) public returns (uint256 x) {
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                x := add(x, i)
            }
        }
    }

    function step_not_visible_in_body() public {
        assembly {
            for { let i := 0 } lt(i, 1) { let j := i } {
                pop(j) //~ ERROR: unresolved symbol
            }
        }
    }

    function body_not_visible_in_step() public {
        assembly {
            for { let i := 0 } lt(i, 1) { pop(j) } {
                //~^ ERROR: unresolved symbol
                let j := i
            }
        }
    }

    function function_in_body_not_visible_in_step() public {
        assembly {
            for { let i := 0 } lt(i, 1) { pop(g()) } {
                //~^ ERROR: unresolved symbol
                function g() -> r {
                    r := i //~ ERROR: unresolved symbol
                }
            }
        }
    }
}
