//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: test_g() => 1, 7
//@[gas] run-call: test_g() => 1, 7
//@[size] run-call: test_g() => 1, 7
//@[none] run-call: test_h() => 43
//@[gas] run-call: test_h() => 43
//@[size] run-call: test_h() => 43

contract C {
    struct S {
        uint v;
    }
    S[] arr;
    uint x;

    function setUp() external {
        arr.push(S(7));
        arr.push(S(8));
    }

    function f() internal view returns (uint, S storage) {
        return (1, arr[0]);
    }

    function test_g() external returns (uint, uint) {
        (x, arr[1]) = f();
        return (x, arr[1].v);
    }

    function test_h() external returns (uint) {
        (uint y, S storage s) = f();
        s.v = 42;
        return y + arr[0].v;
    }
}

