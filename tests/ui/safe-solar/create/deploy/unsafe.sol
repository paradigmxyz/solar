//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: spawn 0x63deadbeef6000526004601cf3 => 0xdeadbeef
//@ run-call: spawnAt 0x63deadbeef6000526004601cf3, 0x0000000000000000000000000000000000000000000000000000000000000001 => 0xdeadbeef
//@ run-call: spawn 0xfe => 0x

// The same creations in assembly. Invalid initcode does not fail here: the
// zero address comes back and its empty code is returned as if it were a
// deployment, which is how a clone factory ends up pointing at nothing.
// CHECK-LABEL: fn @spawn
// CHECK: create
contract Unsafe {
    function spawn(bytes memory initcode) public returns (bytes memory) {
        address deployed;
        assembly ("memory-safe") {
            deployed := create(0, add(initcode, 0x20), mload(initcode))
        }
        return deployed.code;
    }

    function spawnAt(bytes memory initcode, bytes32 salt) public returns (bytes memory) {
        address deployed;
        assembly ("memory-safe") {
            deployed := create2(0, add(initcode, 0x20), mload(initcode), salt)
        }
        return deployed.code;
    }
}
