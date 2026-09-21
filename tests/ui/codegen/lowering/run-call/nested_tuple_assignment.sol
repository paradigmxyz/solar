//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
// ported-from: tests/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow.sol
//@ run-call: assign => 0x30, 0x31, 0x32
//@ run-call: swap => 2, 1, 4, 3
//@ run-call: choose true, 7, 11 => 7, 11
//@ run-call: choose false, 7, 11 => 11, 7

contract NestedTupleAssignment {
    function choose(bool flag, uint256 a, uint256 b) external pure returns (uint256, uint256) {
        return flag ? (a, b) : (b, a);
    }

    function assign() external pure returns (uint256, uint256, uint256) {
        bytes memory a;
        bytes memory b;
        bytes memory c;
        (a, (b, c)) = ("0", ("1", "2"));
        return (uint8(a[0]), uint8(b[0]), uint8(c[0]));
    }

    function swap() external pure returns (uint256, uint256, uint256, uint256) {
        uint256 a = 1;
        uint256 b = 2;
        uint256 c = 3;
        uint256 d = 4;
        (a, (b, (c, d))) = (b, (a, (d, c)));
        return (a, b, c, d);
    }
}
