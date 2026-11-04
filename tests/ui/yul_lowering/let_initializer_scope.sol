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

    function for_init() public pure {
        assembly {
            for { let i := add(i, 1) } lt(i, 1) {} {} //~ ERROR: unresolved symbol
        }
    }

    function switch_case() public pure {
        assembly {
            switch 1
            case 1 {
                let y := y //~ ERROR: unresolved symbol
            }
        }
    }
}
