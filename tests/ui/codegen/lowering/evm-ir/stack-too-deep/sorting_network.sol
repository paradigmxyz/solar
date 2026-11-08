//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@ run-call: sort10 [10, 9, 8, 7, 6, 5, 4, 3, 2, 1], 0 => [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
//@ run-call: sort10 [7, 3, 9, 1, 8, 2, 6, 4, 0, 5, 11, 10], 1 => [7, 0, 1, 2, 3, 4, 5, 6, 8, 9, 11, 10]
//@ run-call: sort10 [4, 4, 1, 1, 3, 3, 2, 2, 0, 0], 0 => [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]
//@ run-call-fail: sort10 [1, 2, 3, 4, 5, 6, 7, 8, 9], 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: sort10 [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A sorting network keeps ten loaded elements on the stack, and common-subexpression elimination
// merges each element's store address with its load address, so twenty words stay live across the
// network. The planner then spills, and the frame it spills to raises the free memory pointer of
// every call that can reach the network. The scheduler instead computes every address from the
// first one loaded, as `base - 32 * k`, at the loads and again at the stores, and the network
// needs no frame.
pragma solidity ^0.8.0;

contract SortingNetworks {
    // CHECK-LABEL: @module SortingNetworks_runtime
    // The heap starts one word above the reserved words, which the return writes its tuple offset
    // into: no spill frame lies below it.
    // Gas mode uses the initial heap base directly when allocating the array.
    // GAS: push 5{{$}}
    // GAS-NEXT: shl
    // GAS-NEXT: push 160{{$}}
    // GAS-NEXT: swap 1
    // GAS-NEXT: dup 2
    // GAS-NEXT: add
    // Size mode initializes the free memory pointer before the allocation.
    // SIZE: push 160{{$}}
    // SIZE-NEXT: push 64
    // SIZE-NEXT: mstore
    function sort10(uint256[] memory a, uint256 low) external pure returns (uint256[] memory) {
        _sort10(a, low);
        return a;
    }

    // The first element loaded is `a[low + 9]`; `a[low]` lies 288 bytes below it.
    // CHECK: push 288
    // CHECK-NEXT: dup {{[0-9]+}}
    // CHECK-NEXT: sub
    // CHECK-NEXT: mload
    // CHECK: push 288
    // CHECK-NEXT: dup {{[0-9]+}}
    // CHECK-NEXT: sub
    // CHECK-NEXT: mstore
    function _sort10(uint256[] memory a, uint256 low) private pure {
        uint256 x9 = a[low + 9];
        uint256 x0 = a[low];
        uint256 x1 = a[low + 1];
        uint256 x2 = a[low + 2];
        uint256 x3 = a[low + 3];
        uint256 x4 = a[low + 4];
        uint256 x5 = a[low + 5];
        uint256 x6 = a[low + 6];
        uint256 x7 = a[low + 7];
        uint256 x8 = a[low + 8];
        if (x8 < x0) (x0, x8) = (x8, x0);
        if (x9 < x1) (x1, x9) = (x9, x1);
        if (x7 < x2) (x2, x7) = (x7, x2);
        if (x5 < x3) (x3, x5) = (x5, x3);
        if (x6 < x4) (x4, x6) = (x6, x4);
        if (x2 < x0) (x0, x2) = (x2, x0);
        if (x4 < x1) (x1, x4) = (x4, x1);
        if (x8 < x5) (x5, x8) = (x8, x5);
        if (x9 < x7) (x7, x9) = (x9, x7);
        if (x3 < x0) (x0, x3) = (x3, x0);
        if (x4 < x2) (x2, x4) = (x4, x2);
        if (x7 < x5) (x5, x7) = (x7, x5);
        if (x9 < x6) (x6, x9) = (x9, x6);
        if (x1 < x0) (x0, x1) = (x1, x0);
        if (x6 < x3) (x3, x6) = (x6, x3);
        if (x9 < x8) (x8, x9) = (x9, x8);
        if (x5 < x1) (x1, x5) = (x5, x1);
        if (x3 < x2) (x2, x3) = (x3, x2);
        if (x8 < x4) (x4, x8) = (x8, x4);
        if (x7 < x6) (x6, x7) = (x7, x6);
        if (x2 < x1) (x1, x2) = (x2, x1);
        if (x5 < x3) (x3, x5) = (x5, x3);
        if (x6 < x4) (x4, x6) = (x6, x4);
        if (x8 < x7) (x7, x8) = (x8, x7);
        if (x3 < x2) (x2, x3) = (x3, x2);
        if (x5 < x4) (x4, x5) = (x5, x4);
        if (x7 < x6) (x6, x7) = (x7, x6);
        if (x4 < x3) (x3, x4) = (x4, x3);
        if (x6 < x5) (x5, x6) = (x6, x5);
        a[low] = x0;
        a[low + 1] = x1;
        a[low + 2] = x2;
        a[low + 3] = x3;
        a[low + 4] = x4;
        a[low + 5] = x5;
        a[low + 6] = x6;
        a[low + 7] = x7;
        a[low + 8] = x8;
        a[low + 9] = x9;
    }
}
