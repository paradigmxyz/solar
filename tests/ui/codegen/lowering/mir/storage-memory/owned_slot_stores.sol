//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// A value that owns its slot is written as the whole word: no read-modify-write keeps the old
// bits above it. The mask below is the one an `address` or `bytes20` store would keep.
interface Ext {
    function get() external returns (address);
}

contract OwnedSlotStores {
    struct S {
        address a;
    }

    address owner;
    bytes20 tag;
    S s;
    address[] dynamic;
    address[3] fixedArray;
    mapping(uint256 => address) owners;
    uint8[1] single;
    uint8[3] triple;

    // CHECK-LABEL: fn @constructor(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 0,
    // CHECK-LABEL: fn @assign(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 0,
    constructor(address a) {
        owner = a;
    }

    function assign(address a, bytes20 b) external {
        owner = a;
        tag = b;
    }

    // CHECK-LABEL: fn @increment(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 0,
    function increment() external {
        owner = address(uint160(owner) + 1);
    }

    // CHECK-LABEL: fn @clear(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 0, 0
    function clear() external {
        delete owner;
    }

    // CHECK-LABEL: fn @structField(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 2,
    function structField(address a) external {
        s.a = a;
    }

    // CHECK-LABEL: fn @structFromMemory(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 2,
    function structFromMemory(address a) external {
        s = S(a);
    }

    // CHECK-LABEL: fn @push(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore
    function push(address a) external {
        dynamic.push(a);
    }

    // CHECK-LABEL: fn @pop(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore
    function pop() external {
        dynamic.pop();
    }

    // CHECK-LABEL: fn @setElement(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore
    function setElement(uint256 i, address a) external {
        dynamic[i] = a;
        fixedArray[i] = a;
    }

    // CHECK-LABEL: fn @copyArray(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore
    function copyArray(address[] memory x) external {
        dynamic = x;
    }

    // CHECK-LABEL: fn @setMapping(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore
    function setMapping(uint256 i, address a) external {
        owners[i] = a;
    }

    // CHECK-LABEL: fn @fromCall(
    // CHECK-NOT: 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
    // CHECK: sstore 0,
    function fromCall(Ext e) external {
        owner = e.get();
    }

    // CHECK-LABEL: fn @setSingle(
    // CHECK-NOT: sload
    // CHECK: sstore
    function setSingle(uint8 x) external {
        single[0] = x;
    }

    // CHECK-LABEL: fn @copyTriple(
    // CHECK-NOT: sload
    // CHECK: sstore
    function copyTriple(uint8[3] memory x) external {
        triple = x;
    }
}
