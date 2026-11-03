contract C {
    function single() public pure {
        assembly {
            let x := add(1, x) //~ ERROR: unresolved symbol
        }
    }

    function multi() public pure {
        assembly {
            function f(p) -> u, v {
                u := p
                v := p
            }
            let a, b := f(b) //~ ERROR: unresolved symbol
        }
    }

    function outer_visible() public pure returns (uint256 r) {
        assembly {
            let x := 1
            {
                let x := add(x, 1)
                r := x
            }
        }
    }
}
