//@ run-call: test_g() => 1, 7
//@ run-call: test_h() => 43
// ported-from: test/libsolidity/semanticTests/types/storage_reference_mixed_tuple.sol

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