//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: carry(uint256) 43 => 60
//@[run] run-call: carry(uint256) 42 => 65
//@[run] run-call: carryAcrossUnevenEdges(uint256) 14 => 19
//@[run] run-call: carryAcrossUnevenEdges(uint256) 15 => 21

contract CrossBlockStackJoin {
    // `kept` is defined before the diamond and reused after its join. Reserve its ordinary spill
    // slot as fallback, but carry the live copy through both predecessors without storing it.
    // CHECK-LABEL: @module runtime
    // CHECK: div
    // CHECK-NEXT: push 1
    // CHECK: jump [[JOIN:bb[0-9]+]]
    // CHECK: jump [[JOIN]]
    // CHECK: [[JOIN]]:
    // The selected phi reloads, but `kept` remains immediately below it instead of loading its
    // own spill slot.
    // CHECK-NEXT: push {{[0-9]+}}
    // CHECK-NEXT: mload
    // CHECK-NEXT: dup2
    function carry(uint256 x) external pure returns (uint256 result) {
        uint256 kept;
        assembly {
            kept := div(x, 7)
        }
        uint256 selected;
        if (x & 1 != 0) {
            selected = x + 1;
        } else {
            selected = x - 1;
        }
        assembly {
            result := add(add(kept, xor(kept, selected)), add(kept, kept))
        }
    }

    // The quotient is live only on the hot arm before the join but must remain reloadable after
    // the cold arm drops its stack copy. This prevents edge-specific residency from suppressing
    // the quotient's only fallback spill store.
    function carryAcrossUnevenEdges(uint256 x) external pure returns (uint256 result) {
        uint256 kept;
        assembly {
            kept := div(x, 7)
        }
        if (x & 1 != 0) {
            assembly {
                result := add(kept, 1)
            }
        } else {
            result = 1;
        }
        assembly {
            result := add(result, mul(kept, 9))
        }
    }
}
