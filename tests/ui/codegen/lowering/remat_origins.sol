//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A value recomputed at its use keeps the origin of the instruction that defines it, not the one of
// the instruction that consumes it.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 68 !metadata(span=[[B:[0-9]+..[0-9]+]])
// CHECK-NEXT: calldataload !metadata(span=[[B]])
// CHECK-NEXT: push 36 !metadata(span=[[A:[0-9]+..[0-9]+]])
// CHECK-NEXT: calldataload !metadata(span=[[A]])
// CHECK-NOT: add !metadata(span=[[A]])
// CHECK-NOT: add !metadata(span=[[B]])
// CHECK: push 4
// CHECK: push 100 !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: calldataload !metadata(span=[[C]])
// CHECK-NOT: add !metadata(span=[[C]])
// CHECK: jump bb7
// CHECK: caller !metadata(span=[[D:[0-9]+..[0-9]+]])
// CHECK-NOT: add !metadata(span=[[D]])
// CHECK: push 0
contract C {
    function f(uint256 x) external view returns (uint256 r) {
        assembly {
            r := add(calldataload(36), calldataload(68))
            if x { r := add(r, calldataload(100)) }
            r := add(r, caller())
        }
    }
}
