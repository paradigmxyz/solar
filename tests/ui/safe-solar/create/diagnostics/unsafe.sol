//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: attempt 0x63deadbeef6000526004601cf3 => true, 0, 0
//@ run-call: attempt 0x7f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 32, 32

// The same in assembly, copying all of what came back into a buffer made for
// four bytes: twenty-eight of them land in whatever follows it.
// CHECK-LABEL: fn @attempt
// CHECK: create
// CHECK: returndatacopy
contract Unsafe {
    function attempt(bytes memory initcode) public returns (bool ok, uint256 copied, uint256 total) {
        bytes memory diagnostics = new bytes(4);
        assembly ("memory-safe") {
            ok := iszero(iszero(create(0, add(initcode, 0x20), mload(initcode))))
            total := returndatasize()
            copied := total
            returndatacopy(add(diagnostics, 0x20), 0, copied)
        }
    }
}
