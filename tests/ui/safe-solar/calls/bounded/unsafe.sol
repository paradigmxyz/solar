//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: probe 0x604d600c600039604d6000f37f11111111111111111111111111111111111111111111111111111111111111116000527f222222222222222222222222222222222222222222222222222222222222222260205260406000f3 => true, 32, 64, 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call: probe 0x6029600c60003960296000f37f333333333333333333333333333333333333333333333333333333333333333360005260206000fd => false, 32, 32, 0x3333333333333333333333333333333333333333333333333333333333333333

// The same call in assembly, as the libraries write it.
// CHECK-LABEL: fn @probe
// CHECK: call {{v[0-9]+}}, {{v[0-9]+}}, 0, 0, 0, {{v[0-9]+}}, 32
// CHECK: returndatasize
contract Unsafe {
    function probe(bytes memory initcode)
        public
        returns (bool ok, uint256 copied, uint256 total, bytes32 first)
    {
        address target;
        assembly ("memory-safe") {
            target := create(0, add(initcode, 0x20), mload(initcode))
        }
        bytes memory out = new bytes(32);
        assembly ("memory-safe") {
            ok := call(gas(), target, 0, 0, 0, add(out, 0x20), 32)
            total := returndatasize()
            copied := total
            if gt(copied, 32) { copied := 32 }
            first := mload(add(out, 0x20))
        }
    }
}
