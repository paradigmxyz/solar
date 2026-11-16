//@ codegen-matrix: standard
//@ run-call: C::leaveInFor => 5
//@ run-call: C::leaveEarly => 123
//@ run-call: C::leaveMulti => 7, 0

// Yul `leave` lowers to a bare `return;`, and both must deliver the declared
// return variables' current values — not defaults. The bare-return path once
// emitted `ret []`, so every `leave`-based Yul helper (solady's JSONParserLib
// `skipWhitespace`, LibRLP, LibZip) returned zero pointers. Solidity-level
// `return;` with named returns takes the same path, but sema currently
// rejects it (solc accepts) — covered here once that lands.

contract C {
    function leaveInFor() external pure returns (uint256 r) {
        assembly {
            function skip(p) -> out {
                for { out := p } 1 { out := add(out, 1) } {
                    if eq(out, 5) { leave }
                }
            }
            r := skip(2)
        }
    }

    function leaveEarly() external pure returns (uint256 r) {
        assembly {
            function f(a) -> out {
                out := a
                if gt(a, 10) { leave }
                out := add(a, 100)
            }
            r := add(f(20), f(3))
        }
    }

    function leaveMulti() external pure returns (uint256 x, uint256 y) {
        assembly {
            function pair(a) -> o1, o2 {
                o1 := a
                if gt(a, 5) { leave }
                o2 := add(a, 1)
            }
            x, y := pair(7)
        }
    }

}
