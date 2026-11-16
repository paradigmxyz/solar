//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test_g => 1, 7
//@ run-call: test_h => 43
//@ run-call: mapping_reference => 42, 0, 0, 21

contract C {
    struct S {
        uint v;
    }
    S[] arr;
    uint x;
    mapping(uint8 => uint8) first;
    mapping(uint8 => uint8) second;

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

    function mapping_reference() external returns (uint8, uint8, uint8, uint8) {
        mapping(uint8 => uint8) storage current = first;
        current[1] = 42;

        uint8 value;
        (current, value) = (second, 21);
        current[2] = value;

        return (first[1], first[2], second[1], second[2]);
    }
}
