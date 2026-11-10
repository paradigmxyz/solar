//@ compile-flags: -Ogas -Zcheck-analyses=true -Zdump=mir
//@ filecheck:
//@ run-call: next 5 => 6
//@ run-call: next 9 => 10
//@ run-call: next 20 => 0
//@ run-call: bump => 1

// Check elimination proves `x + 1` cannot overflow under `x < 10` and drops the
// overflow test, recording that the test lowering would emit is false. CSE
// forwards the stored `total` to the read after it, recording that the read
// equals the stored value. `lower-assumes` tests both records: fact words
// `0x102...` and `0x103...` name `eq` (1) and the facts check elimination (2)
// and CSE (3) relied on.
contract Recorded {
    uint256 public total;

    // CHECK-LABEL: fn @bump(
    // CHECK: phi [{{.*}}: {{0x103[0-9a-f]+}}]
    // CHECK: mstore 0, 0xed572b27
    function bump() external returns (uint256) {
        total = total + 1;
        return total;
    }

    // CHECK-LABEL: fn @next(
    // CHECK: phi [{{.*}}: {{0x102[0-9a-f]+}}]
    // CHECK: mstore 0, 0xed572b27
    function next(uint256 x) external pure returns (uint256) {
        if (x < 10) {
            return x + 1;
        }
        return 0;
    }
}
