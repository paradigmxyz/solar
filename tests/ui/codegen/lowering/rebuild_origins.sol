//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A value rebuilt at its use instead of carried across a crowded stack keeps the origin of its
// definition, not the one of the instruction that reads it.

// CHECK-LABEL: (scheduled runtime) ===
// A chain of operations that is not a sum is rebuilt the same way (`g`, laid out first).
// CHECK: push 514 !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: push 4 !metadata(span=[[C]])
// CHECK-NEXT: calldataload !metadata(span=[[C]])
// CHECK-NEXT: xor !metadata(span=[[C]])
// CHECK-NOT: add !metadata(span=[[C]])
// CHECK: push 771
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

    function g(uint256 value) external pure returns (uint256) {
        unchecked {
            uint256 b0 = value ^ 257;
            uint256 b1 = value ^ 514;
            uint256 b2 = value ^ 771;
            uint256 b3 = value ^ 1028;
            uint256 b4 = value ^ 1285;
            uint256 b5 = value ^ 1542;
            uint256 b6 = value ^ 1799;
            uint256 b7 = value ^ 2056;
            uint256 b8 = value ^ 2313;
            uint256 b9 = value ^ 2570;
            uint256 b10 = value ^ 2827;
            uint256 b11 = value ^ 3084;
            uint256 b12 = value ^ 3341;
            uint256 b13 = value ^ 3598;
            uint256 b14 = value ^ 3855;
            uint256 b15 = value ^ 4112;
            uint256 b16 = value ^ 4369;
            uint256 b17 = value ^ 4626;
            uint256 mid = value * value;
            return mid + b0 + b1 + b2 + b3 + b4 + b5 + b6 + b7 + b8 + b9 + b10 + b11 + b12 + b13 + b14 + b15 + b16 + b17;
        }
    }
}
