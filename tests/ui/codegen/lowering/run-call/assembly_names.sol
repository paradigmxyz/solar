//@ run-call: shadowedReturn => 42
//@ run-call: shadowedArgument 1 => 7
//@ run-call: reservedNames => 6

// solc rejects these Yul declarations with errors 3859, 4113, 5017 and 6578; see TYPECK-012 in
// docs/SOLC_DIVERGENCE.md.
contract AssemblyNames {
    function shadowedReturn() external pure returns (uint256 x) {
        assembly {
            function g() -> x {
                x := 42
            }
            x := g()
        }
    }

    function shadowedArgument(uint256 a) external pure returns (uint256 r) {
        assembly {
            let a := 7
            r := a
        }
    }

    function reservedNames() external pure returns (uint256 r) {
        assembly {
            function this() -> _ {
                _ := 1
            }
            let pc := 2
            let dup1 := 3
            r := add(add(this(), pc), dup1)
        }
    }
}
