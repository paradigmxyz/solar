//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: relay => 1
//@ run-call-fail: relayLocked()
//@ run-call: relayThis => 0

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
}
