//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: pairA 10 => 10, 11
//@ run-call: pairB 20 => 20, 21
//@ run-call: lone 30 => 30

type Builder is uint256;

function newBuilder() pure returns (Builder b) {
    assembly ("memory-safe") {
        b := mload(0x40)
        mstore(0x40, add(0x20, b))
    }
}

function singleBuilder() pure returns (Builder b) {
    assembly ("memory-safe") {
        b := mload(0x40)
        mstore(0x40, add(0x20, b))
    }
}

function writeWord(Builder b, uint256 value) pure {
    assembly ("memory-safe") {
        mstore(b, value)
    }
}

function readWord(Builder b) pure returns (uint256 value) {
    assembly ("memory-safe") {
        value := mload(b)
    }
}

// A multi-use helper that bumps `0x40` and returns a value-type handle stays shared under `-Ogas`,
// while a sole-use one still inlines.
contract AllocatingLeaf {
    // CHECK-LABEL: fn @pairA()
    // CHECK: icall @newBuilder
    // CHECK: icall @newBuilder
    function pairA(uint256 x) external pure returns (uint256, uint256) {
        Builder a = newBuilder();
        Builder b = newBuilder();
        writeWord(a, x);
        writeWord(b, x + 1);
        return (readWord(a), readWord(b));
    }

    // CHECK-LABEL: fn @pairB()
    // CHECK: icall @newBuilder
    // CHECK: icall @newBuilder
    function pairB(uint256 x) external pure returns (uint256, uint256) {
        Builder a = newBuilder();
        Builder b = newBuilder();
        writeWord(a, x);
        writeWord(b, x + 1);
        return (readWord(a), readWord(b));
    }

    // CHECK-LABEL: fn @lone()
    // CHECK-NOT: icall @singleBuilder
    function lone(uint256 x) external pure returns (uint256) {
        Builder a = singleBuilder();
        writeWord(a, x);
        return readWord(a);
    }
}

// CHECK-LABEL: fn @newBuilder() -> i256
// CHECK-NOT: fn @singleBuilder
