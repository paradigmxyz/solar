//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
// ported-from: tests/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow.sol
//@[none, gas, size] run-call: assign() => 0x30, 0x31, 0x32
//@[none, gas, size] run-call: swap() => 2, 1, 4, 3

contract NestedTupleAssignment {
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
