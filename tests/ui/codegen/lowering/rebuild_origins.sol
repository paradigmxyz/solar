//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:

// A value rebuilt at its use instead of carried across a crowded stack keeps the origin of its
// definition, not the one of the instruction that reads it.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 3 !metadata(span=[[A:[0-9]+..[0-9]+]])
// CHECK-NEXT: dup 3 !metadata(span=[[A]])
// CHECK-NEXT: add !metadata(span=[[A]])
// CHECK-NOT: add !metadata(span=[[A]])
// CHECK: push 4 !metadata(span=[[B:[0-9]+..[0-9]+]])
// CHECK-NEXT: dup 3 !metadata(span=[[B]])
// CHECK-NEXT: add !metadata(span=[[B]])
// CHECK-NOT: add !metadata(span=[[B]])
// CHECK: push 5
contract C {
    function f(uint256 value) external pure returns (uint256) {
        unchecked {
            uint256 a0 = value + 1;
            uint256 a1 = value + 2;
            uint256 a2 = value + 3;
            uint256 a3 = value + 4;
            uint256 a4 = value + 5;
            uint256 a5 = value + 6;
            uint256 a6 = value + 7;
            uint256 a7 = value + 8;
            uint256 a8 = value + 9;
            uint256 a9 = value + 10;
            uint256 a10 = value + 11;
            uint256 a11 = value + 12;
            uint256 a12 = value + 13;
            uint256 a13 = value + 14;
            uint256 a14 = value + 15;
            uint256 a15 = value + 16;
            uint256 a16 = value + 17;
            uint256 a17 = value + 18;
            uint256 mid = value * value;
            return mid + a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17;
        }
    }
}
