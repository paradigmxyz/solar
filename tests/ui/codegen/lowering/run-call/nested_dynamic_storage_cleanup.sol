//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f() => 0, 0

contract NestedDynamicStorageCleanup {
    uint256[][] private x;

    function f() external returns (uint256 length, uint256 data) {
        x.push();
        x[0].push(7);
        x[0].push(8);
        x.push();
        x[1].push(9);

        uint256[][] memory replacement = new uint256[][](1);
        replacement[0] = new uint256[](1);
        replacement[0][0] = 23;
        x = replacement;
        assembly {
            sstore(x.slot, 2)
            mstore(0, x.slot)
            let outer := keccak256(0, 0x20)
            mstore(0, add(outer, 1))
            let inner := keccak256(0, 0x20)
            length := sload(add(outer, 1))
            data := sload(inner)
        }
    }
}
