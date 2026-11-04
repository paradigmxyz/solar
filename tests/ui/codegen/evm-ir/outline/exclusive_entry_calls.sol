//@ compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: a 0x0000000000000000000000000000000000000005 => 6
//@ run-call: b 0x0000000000000000000000000000000000000005 => 7
//@ run-call: c 0x0000000000000000000000000000000000000005 => 8
//@ run-call: d 0x0000000000000000000000000000000000000005 => 9
//@ run-call: e 0x0000000000000000000000000000000000000005 => 10

// Each external function builds the same `getReserveData` call arguments. A call runs one of them,
// so sharing the argument setup costs one jump pair per call, not five, and gas mode shares it
// between the entries.
// CHECK: function a(address x)
// CHECK: jump [[ARGS:bb[0-9]+]]
// CHECK: [[ARGS]] {{\[ret=0, inputs=1, outputs=[0-9]+\]}}:
// CHECK-COUNT-4: jump [[ARGS]]
contract ExclusiveEntryCalls {
    function getReserveData(address asset) external pure returns (uint256, uint256, uint256, uint256) {
        return (uint160(asset), 2, 3, 4);
    }

    function a(address x) external view returns (uint256) {
        (uint256 q,,,) = ExclusiveEntryCalls(address(this)).getReserveData(x);
        return q + 1;
    }

    function b(address x) external view returns (uint256) {
        (uint256 q,,,) = ExclusiveEntryCalls(address(this)).getReserveData(x);
        return q + 2;
    }

    function c(address x) external view returns (uint256) {
        (uint256 q,,,) = ExclusiveEntryCalls(address(this)).getReserveData(x);
        return q + 3;
    }

    function d(address x) external view returns (uint256) {
        (uint256 q,,,) = ExclusiveEntryCalls(address(this)).getReserveData(x);
        return q + 4;
    }

    function e(address x) external view returns (uint256) {
        (uint256 q,,,) = ExclusiveEntryCalls(address(this)).getReserveData(x);
        return q + 5;
    }
}
