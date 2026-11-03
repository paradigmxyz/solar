//@ codegen-matrix: standard
//@ run-call: shadowedLocal => 6
//@ run-call: shadowedMulti => 20, 10
//@ run-call: nestedBlock => 5
//@ run-call: shadowedByFunction => 31

// The initializer of a Yul `let` does not see the names it declares, so it reads the shadowed outer
// declaration. solc rejects the Yul declarations that shadow Solidity ones; see TYPECK-013 in
// docs/SOLC_DIVERGENCE.md.
contract AssemblyLetInitializerScope {
    function shadowedLocal() external pure returns (uint256 r) {
        uint256 x = 5;
        assembly {
            let x := add(x, 1)
            r := x
        }
    }

    function shadowedMulti() external pure returns (uint256 r, uint256 s) {
        uint256 a = 10;
        uint256 b = 20;
        assembly {
            function swap(p, q) -> u, v {
                u := q
                v := p
            }
            let a, b := swap(a, b)
            r := a
            s := b
        }
    }

    function nestedBlock() external pure returns (uint256 r) {
        assembly {
            let x := 1
            {
                let x := add(x, 1)
                r := x
            }
            r := add(r, mul(x, 3))
        }
    }

    function shadowedByFunction() external pure returns (uint256 r) {
        uint256 x = 30;
        assembly {
            function one() -> o {
                o := 1
            }
            let x := add(x, one())
            r := x
        }
    }
}
