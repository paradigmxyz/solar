//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test false, 17 => 17, 0
//@ run-call: test true, 19 => 0, 19

contract StorageReturnPointerDirect {
    struct S {
        uint256 x;
    }

    S a;
    S b;

    function pick(bool useB) internal view returns (S storage result) {
        if (useB) return b;
        return a;
    }

    function test(bool useB, uint256 value) external returns (uint256, uint256) {
        pick(useB).x = value;
        return (a.x, b.x);
    }
}
