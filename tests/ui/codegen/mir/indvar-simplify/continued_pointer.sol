//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: fill() => [8, 8, 9, 9, 9, 9]
//@ run-call: findMemory [4, 5, 6], 5 => 5
//@ run-call: findMemory [4, 5, 6], 7 => 0
//@ run-call: findMemory [], 7 => 0
//@ run-call: findCalldata [4, 5, 6], 6 => 6
//@ run-call: findCalldata [4, 5, 6], 7 => 0
//@ run-call: findCalldata [], 7 => 0
//@ run-call: pairOf [4, 5, 6], [7, 8, 9], 5 => 8
//@ run-call: pairOf [4, 5, 6], [7, 8, 9], 1 => 0
//@ run-call: pairFrom [4, 5, 6], [7, 8, 9], 6 => 9
//@ run-call: pairFrom [4, 5, 6], [7, 8, 9], 4 => 0
//@ run-call: pairFrom [], [], 4 => 0
pragma solidity ^0.8.0;

// Once a pointer takes over its loop's exit test, the counter is rebuilt after
// the loop as `(ptr - start) >> 5`. A read that scales it back into the same
// address family, the element a search stopped at or the start of a loop that
// continues the walk, takes the pointer itself instead of that round trip.
contract ContinuedPointer {
    // The second loop's pointer starts where the first one stopped, although
    // each loop hoists its own copy of the array's data offset.
    // CHECK-LABEL: fn @fill
    // CHECK: [[FIRST:v[0-9]+]] = phi [bb{{[0-9]+}}: {{v[0-9]+}}], [bb{{[0-9]+}}: {{v[0-9]+}}]
    // CHECK-NOT: shr
    // CHECK: phi [bb{{[0-9]+}}: [[FIRST]]], [bb{{[0-9]+}}: {{v[0-9]+}}]
    function fill() public pure returns (uint256[] memory a) {
        a = new uint256[](6);
        uint256 i;
        for (; i < 2; i++) {
            a[i] = 8;
        }
        for (; i < 6; i++) {
            a[i] = 9;
        }
    }

    // The element a search stopped at is the element its pointer stopped at.
    // CHECK-LABEL: fn @findMemory
    // CHECK: [[PTR:v[0-9]+]] = phi [bb{{[0-9]+}}: {{v[0-9]+}}], [bb{{[0-9]+}}: {{v[0-9]+}}]
    // CHECK: bb5:
    // CHECK-NEXT: mload [[PTR]]
    function findMemory(uint256[] memory a, uint256 x) public pure returns (uint256) {
        uint256 i;
        for (; i < a.length; i++) {
            if (a[i] == x) break;
        }
        return i < a.length ? a[i] : 0;
    }

    // CHECK-LABEL: fn @findCalldata
    // CHECK: [[PTR:v[0-9]+]] = phi [bb{{[0-9]+}}: {{v[0-9]+}}], [bb{{[0-9]+}}: {{v[0-9]+}}]
    // CHECK: bb5:
    // CHECK-NEXT: calldataload [[PTR]]
    function findCalldata(uint256[] calldata a, uint256 x) external pure returns (uint256) {
        uint256 i;
        for (; i < a.length; i++) {
            if (a[i] == x) break;
        }
        return i < a.length ? a[i] : 0;
    }

    // Another array's element is outside the pointer's family; it scales the
    // counter back to the pointer's distance from its start without shifting.
    // CHECK-LABEL: fn @pairOf
    // CHECK: [[PTR:v[0-9]+]] = phi [bb{{[0-9]+}}: [[START:v[0-9]+]]], [bb{{[0-9]+}}: {{v[0-9]+}}]
    // CHECK: bb7:
    // CHECK-NEXT: [[DELTA:v[0-9]+]] = sub [[PTR]], [[START]]
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add {{v[0-9]+}}, [[DELTA]]
    // CHECK-NEXT: calldataload [[ADDRESS]]
    function pairOf(uint256[] calldata a, uint256[] calldata b, uint256 x)
        external
        pure
        returns (uint256)
    {
        uint256 i;
        for (; i < a.length; i++) {
            if (a[i] == x) break;
        }
        return i < a.length && i < b.length ? b[i] : 0;
    }

    // A search from the second element counts its distance from one element in.
    // CHECK-LABEL: fn @pairFrom
    // CHECK: [[PTR:v[0-9]+]] = phi [bb{{[0-9]+}}: [[START:v[0-9]+]]], [bb{{[0-9]+}}: {{v[0-9]+}}]
    // CHECK: bb7:
    // CHECK-NEXT: [[DELTA:v[0-9]+]] = sub [[PTR]], [[START]]
    // CHECK-NEXT: [[DISTANCE:v[0-9]+]] = add [[DELTA]], 32
    // CHECK-NEXT: [[ADDRESS:v[0-9]+]] = add {{v[0-9]+}}, [[DISTANCE]]
    // CHECK-NEXT: calldataload [[ADDRESS]]
    function pairFrom(uint256[] calldata a, uint256[] calldata b, uint256 x)
        external
        pure
        returns (uint256)
    {
        uint256 i = 1;
        for (; i < a.length; i++) {
            if (a[i] == x) break;
        }
        return i < a.length && i < b.length ? b[i] : 0;
    }
}
