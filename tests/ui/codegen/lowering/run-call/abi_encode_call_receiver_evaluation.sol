//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: relay => 1
//@ run-call-fail: relayLocked()
//@ run-call: relayThis => 0
//@ run-call: selector => 1
//@ run-call: functionAddress => 1
//@ run-call: encodeWithSelector => 1
//@ run-call: pointer => 1
//@ run-call: tryCall => 1

contract Vault {
    uint256 public n;

    function gate() internal returns (Vault) {
        n += 1;
        return this;
    }

    function locked() internal view returns (Vault) {
        require(n != 0);
        return this;
    }

    function payout(address to, uint256 amount) external {}

    // CHECK-LABEL: fn @relay()
    // CHECK: icall @gate
    // CHECK-NEXT: abi_encode {{.*}} selector 0x117de2fd
    function relay() external returns (uint256) {
        abi.encodeCall(gate().payout, (address(0), 1));
        return n;
    }

    // CHECK-LABEL: fn @relayLocked()
    // CHECK: icall @locked
    // CHECK-NEXT: abi_encode {{.*}} selector 0x117de2fd
    function relayLocked() external view returns (bytes memory) {
        return abi.encodeCall(locked().payout, (address(0), 1));
    }

    // CHECK-LABEL: fn @relayThis()
    // CHECK-NOT: call
    // CHECK: abi_encode {{.*}} selector 0x117de2fd
    function relayThis() external view returns (uint256) {
        abi.encodeCall(this.payout, (address(0), 1));
        return n;
    }

    function selector() external returns (uint256) {
        gate().payout.selector;
        return n;
    }

    function functionAddress() external returns (uint256) {
        gate().payout.address;
        return n;
    }

    function encodeWithSelector() external returns (uint256) {
        abi.encodeWithSelector(gate().payout.selector, address(0), 1);
        return n;
    }

    function pointer() external returns (uint256) {
        function(address, uint256) external f = gate().payout;
        abi.encodeCall(f, (address(0), 1));
        return n;
    }

    function tryCall() external returns (uint256) {
        try gate().payout(address(0), 1) {} catch {}
        return n;
    }
}
