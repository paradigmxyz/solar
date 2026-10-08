//@ compile-flags: -O none -Zdump=evm-ir-runtime
//@ filecheck:

// Without optimization, the ternary after the endless loop in `first` stays
// in the MIR as unreachable blocks, and its phi still gets copies planned on
// their edges. Those blocks are never emitted, so their copies must be dropped
// with the function. Otherwise a later function's block with the same number
// emits them as stray stores, here in the guards of `second`.
contract UnreachablePhiCopies {
    function first(uint256 x) external pure returns (uint256 r) {
        for (;;) {
            if (x > 3) {
                return x;
            }
            x += 1;
        }
        r = x > 1 ? 5 : 6;
    }

    // CHECK-LABEL: {{^}}  push 10{{$}}
    // CHECK-NOT: mstore
    // CHECK: {{^}}  push 15{{$}}
    // CHECK-NOT: mstore
    // CHECK: jumpi
    function second(uint256 x) external pure returns (uint256) {
        require(x != 10);
        require(x != 11);
        require(x != 12);
        require(x != 13);
        require(x != 14);
        require(x != 15);
        return x + 1;
    }
}
