//@ run-call: shadowedReturn => 42
//@ run-call: shadowedArgument 1 => 7
//@ run-call: shadowedOuterNames => 10
//@ run-call: reservedNames => 6

// solc rejects these Yul declarations with errors 3859, 4113, 5017 and 6578; see TYPECK-013 in
// docs/SOLC_DIVERGENCE.md.
contract AssemblyNames {
    uint256 s;

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

    function shadowedOuterNames() external pure returns (uint256 r) {
        assembly {
            let AssemblyNames := 1
            let shadowedOuterNames := 2
            let s := 3
            let msg := 4
            r := add(add(AssemblyNames, shadowedOuterNames), add(s, msg))
        }
    }

    function reservedNames() external pure returns (uint256 r) {
        assembly {
            function this() -> _ {
                _ := 1
            }
            function super(datasize) -> pc {
                pc := datasize
            }
            let dup1 := 3
            r := add(add(this(), super(2)), dup1)
        }
    }
}
