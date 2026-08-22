//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test(bool,uint256) false, 17 => 17, 0
//@ run-call: test(bool,uint256) true, 19 => 0, 19

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
