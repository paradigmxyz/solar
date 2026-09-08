//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: carry 43 => 60
//@ run-call: carry 42 => 65
//@ run-call: carryAcrossUnevenEdges 14 => 19
//@ run-call: carryAcrossUnevenEdges 15 => 21
//@ run-call: carryWithoutPhi 42 => 12
//@ run-call: carryWithoutPhi 43 => 12

contract CrossBlockStackJoin {
    // Both predecessors carry `kept` and the selected phi through the join without reloading.
    // CHECK-LABEL: @module CrossBlockStackJoin_runtime
    // CHECK: div
    // CHECK-NEXT: push 1
    // CHECK-NOT: mstore
    // CHECK: jump [[JOIN:bb[0-9]+]]
    // CHECK-NEXT: [[JOIN]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: add
    // CHECK: jump [[JOIN]]
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

contract PhiFreeStackJoin {
    // Carry the quotient through both edges without introducing a phi or a spill slot.
    // CHECK-LABEL: @module PhiFreeStackJoin_runtime
    // CHECK-NOT: mload
    // CHECK: return
    function carryWithoutPhi(uint256 x) external returns (uint256 result) {
        assembly {
            let kept := div(x, 7)
            if and(x, 1) { sstore(0, kept) }
            result := add(kept, kept)
        }
    }
}
