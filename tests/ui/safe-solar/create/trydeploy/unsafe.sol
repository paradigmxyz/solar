//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: attempt 0x63deadbeef6000526004601cf3 => true, 0xdeadbeef
//@ run-call: attempt 0xfe => false, 0x

// The same creation in assembly, with the flag derived by hand.
// CHECK-LABEL: fn @attempt
// CHECK: create
contract Unsafe {
    function attempt(bytes memory initcode) public returns (bool ok, bytes memory code) {
        address deployed;
        assembly ("memory-safe") {
            deployed := create(0, add(initcode, 0x20), mload(initcode))
            ok := iszero(iszero(deployed))
        }
        code = deployed.code;
    }
}
