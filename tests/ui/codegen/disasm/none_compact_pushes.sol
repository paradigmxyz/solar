//@ compile-flags: -O none -Zdump=disasm-runtime
//@ filecheck: --implicit-check-not=PUSH16

// `-O none` runs `compact-pushes` with gas pricing: a mask outside every loop becomes a shifted
// `PUSH0 NOT`, while a mask in a function that a loop calls stays a literal, which it would
// otherwise rebuild on every iteration. So does a mask in a recursive function.
contract NoneCompactPushes {
    // CHECK: NOT
    // CHECK-NEXT: PUSH1 0x80
    // CHECK-NEXT: SHR
    function once(uint256 x) external pure returns (uint256) {
        return x & 0xffffffffffffffffffffffffffffffff;
    }

    // CHECK: PUSH15 0xffffffffffffffffffffffffffffff
    function masked(uint256 x) internal pure returns (uint256) {
        return x & 0xffffffffffffffffffffffffffffff;
    }

    // CHECK: PUSH15 0xffffffffffffffffffffffffffffff
    function recursive(uint256 n, uint256 x) internal pure returns (uint256) {
        return n == 0 ? x & 0xffffffffffffffffffffffffffffff : recursive(n - 1, x);
    }

    function entry(uint256 n, uint256 x) external pure returns (uint256) {
        return recursive(n, x);
    }

    function looped(uint256 n) external pure returns (uint256 sum) {
        for (uint256 i; i < n; ++i) {
            sum += masked(i);
        }
    }
}
