//@ filecheck:
//@ codegen-matrix: standard
//@ run-call: CheckedBaseArgAdd::x; constructor=[1] => 2
//@ run-call: CheckedBaseArgMul::x; constructor=[3] => 6
//@ run-call: CheckedBaseArgList::x => 6
//@ run-call: CheckedBaseArgElement::x; constructor=[1] => 6
//@ run-call: CheckedBaseArgNegate::y; constructor=[1] => -1
//@ run-call: CheckedBaseArgSub::x; constructor=[3] => 2
//@ run-call: CheckedBaseArgExp::x; constructor=[3] => 9
//@ run-call: CheckedBaseArgFactory::deployShift 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call-fail: CheckedBaseArgFactory::deployAdd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x11)
//@ run-call-fail: CheckedBaseArgFactory::deployMul 0x8000000000000000000000000000000000000000000000000000000000000000 => Panic(0x11)
//@ run-call-fail: CheckedBaseArgFactory::deployNegate -57896044618658097711785492504343953926634992332820282019728792003956564819968 => Panic(0x11)
//@ run-call-fail: CheckedBaseArgFactory::deployElement 2 => Panic(0x32)
//@ run-call-fail: CheckedBaseArgFactory::deploySub 0 => Panic(0x11)
//@ run-call-fail: CheckedBaseArgFactory::deployExp 0x0100000000000000000000000000000000 => Panic(0x11)

contract CheckedBase {
    uint256 public x;

    constructor(uint256 v) {
        x = v;
    }

    function two() public pure returns (uint256) {
        return 2;
    }
}

contract CheckedSignedBase {
    int256 public y;

    constructor(int256 v) {
        y = v;
    }
}

// CHECK-LABEL: @module CheckedBaseArgAdd
// CHECK: fn @constructor(arg0: u256)
// CHECK: [[SUM:v[0-9]+]] = add arg0, 1
// CHECK-NEXT: [[OVERFLOW:v[0-9]+]] = lt [[SUM]], arg0
// CHECK-NEXT: jumpi [[OVERFLOW]], bb[[PANIC:[0-9]+]]
// CHECK: sstore 0, [[SUM]]
// CHECK: bb[[PANIC]]:
// CHECK-NEXT: mstore 0, 0x4e487b71
// CHECK-NEXT: mstore 4, 17
contract CheckedBaseArgAdd is CheckedBase {
    constructor(uint256 v) CheckedBase(v + 1) {}
}

contract CheckedBaseArgMul is CheckedBase {
    constructor(uint256 v) CheckedBase(v * 2) {}
}

contract CheckedBaseArgList is CheckedBase(CheckedBase.two() * 3) {}

contract CheckedBaseArgNegate is CheckedSignedBase {
    constructor(int256 v) CheckedSignedBase(-v) {}
}

contract CheckedBaseArgSub is CheckedBase {
    constructor(uint256 v) CheckedBase(v - 1) {}
}

contract CheckedBaseArgExp is CheckedBase {
    constructor(uint256 v) CheckedBase(v ** 2) {}
}

// A shift truncates instead of overflowing, so it must stay unchecked.
// CHECK-LABEL: @module CheckedBaseArgShift
// CHECK: fn @constructor(arg0: u256)
// CHECK-NEXT: bb0:
// CHECK-NEXT: [[SHIFTED:v[0-9]+]] = shl 1, arg0
// CHECK-NEXT: sstore 0, [[SHIFTED]]
contract CheckedBaseArgShift is CheckedBase {
    constructor(uint256 v) CheckedBase(v << 1) {}
}

contract CheckedBaseArgElement is CheckedBase {
    constructor(uint256 i) CheckedBase(table()[i]) {}

    function table() internal pure returns (uint256[2] memory t) {
        t[0] = 5;
        t[1] = 6;
    }
}

contract CheckedBaseArgFactory {
    function deployAdd(uint256 v) external returns (uint256) {
        return new CheckedBaseArgAdd(v).x();
    }

    function deployMul(uint256 v) external returns (uint256) {
        return new CheckedBaseArgMul(v).x();
    }

    function deployNegate(int256 v) external returns (int256) {
        return new CheckedBaseArgNegate(v).y();
    }

    function deployElement(uint256 i) external returns (uint256) {
        return new CheckedBaseArgElement(i).x();
    }

    function deploySub(uint256 v) external returns (uint256) {
        return new CheckedBaseArgSub(v).x();
    }

    function deployExp(uint256 v) external returns (uint256) {
        return new CheckedBaseArgExp(v).x();
    }

    function deployShift(uint256 v) external returns (uint256) {
        return new CheckedBaseArgShift(v).x();
    }
}
