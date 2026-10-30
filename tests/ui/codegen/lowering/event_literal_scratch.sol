//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: logged 3 => 0

// The log reads an event's data at once, so the data is encoded past the free memory pointer
// without reserving it, as solc does, and a string literal among the arguments takes no object
// of its own: a loop of such events leaves the free memory pointer where it was.
contract EventLiteralScratch {
    event Named(string name, uint256 value);

    // CHECK-LABEL: fn @logged(
    // CHECK-NOT: mstore 64
    // CHECK: [[LENGTH:v[0-9]+]] = heap_floor_offset 64
    // CHECK-NEXT: [[DATA:v[0-9]+]] = heap_floor_offset 96
    // CHECK-NOT: mstore 64
    // CHECK: mstore [[LENGTH]], 17
    // CHECK-NEXT: mstore [[DATA]], 0x7374616e646172644e6f726d616c576164
    // CHECK-NOT: mstore 64
    // CHECK: log {{v[0-9]+}}, 128,
    function logged(uint256 n) external returns (uint256 growth) {
        uint256 start;
        assembly {
            start := mload(0x40)
        }
        for (uint256 i; i < n; ++i) {
            emit Named("standardNormalWad", i);
        }
        assembly {
            growth := sub(mload(0x40), start)
        }
    }
}
