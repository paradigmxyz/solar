//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: declared() => 7132
//@[none, gas, size] run-call: assigned() => 71122

contract MixedMemoryTuple {
    struct S {
        uint256 a;
        uint256 b;
    }

    S internal s1;
    uint256 internal q;

    function f() internal view returns (uint256, S storage ref) {
        ref = s1;
        return (7, ref);
    }

    // A storage-reference return assigned to a memory element copies the
    // value at assignment time; later storage writes must not show through.
    function declared() external returns (uint256) {
        s1 = S(11, 22);
        (uint256 y, S memory m) = f();
        s1.a = 99;
        return y * 1000 + m.a * 10 + m.b;
    }

    function assigned() external returns (uint256) {
        s1 = S(11, 22);
        S memory m = S(1, 2);
        (q, m) = f();
        s1.b = 77;
        return q * 10000 + m.a * 100 + m.b;
    }
}
