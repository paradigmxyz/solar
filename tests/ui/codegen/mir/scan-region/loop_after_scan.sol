//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: sqrt 0 => 2
//@ run-call: sqrt 5 => 8
//@ run-call: sqrt 0x100000000000000000000000000000000000000000000000000 => 0x100000000000000000000000000000000000000000000000000
//@ run-call: sqrt 0x100000000000000000000000000000000000000000000000001 => 0x200000000000000000000000000000000000000000000000000

// A do-while loop right after the search is the block where every run stops, and its own
// predecessor. A region may not loop back from its last block, so the search stays and the loop
// phi keeps reading the new value on the back edge.
contract LoopAfterScan {
    // CHECK-LABEL: fn @sqrt(
    // CHECK-NOT: clz
    // CHECK: ret
    function sqrt(uint256 a) external pure returns (uint256 xn) {
        unchecked {
            uint256 aa = a;
            xn = 1;
            if (aa >= (1 << 128)) { aa >>= 128; xn <<= 64; }
            if (aa >= (1 << 64)) { aa >>= 64; xn <<= 32; }
            if (aa >= (1 << 32)) { aa >>= 32; xn <<= 16; }
            if (aa >= (1 << 16)) { aa >>= 16; xn <<= 8; }
            if (aa >= (1 << 8)) { aa >>= 8; xn <<= 4; }
            if (aa >= (1 << 4)) { aa >>= 4; xn <<= 2; }
            if (aa >= (1 << 2)) { xn <<= 1; }
            do {
                xn <<= 1;
            } while (xn < a);
        }
    }
}
