//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
// ported-from: tests/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow.sol
//@ run-call: assign => 0x30, 0x31, 0x32
//@ run-call: swap => 2, 1, 4, 3

//@ run-call: signedChoice true => -1, 7
//@ run-call: signedChoice false => 256, 9
//@ run-call: unsignedChoice true => 255, true
//@ run-call: unsignedChoice false => 256, false
//@ run-call: arrayChoice true => 7, -1
//@ run-call: arrayChoice false => 9, 256
//@ run-call: sliceChoice true => 7, -1
//@ run-call: sliceChoice false => 9, 256

contract NestedTupleAssignment {
    function signedChoice(bool choose) external pure returns (int16, uint256) {
        return choose ? (int8(-1), uint256(7)) : (int16(256), uint256(9));
    }

    function unsignedChoice(bool choose) external pure returns (uint16, bool) {
        return choose ? (uint8(255), true) : (uint16(256), false);
    }

    function arrayChoice(bool choose) external pure returns (uint256, int16) {
        uint256[] memory first = new uint256[](1);
        uint256[] memory second = new uint256[](1);
        first[0] = 7;
        second[0] = 9;
        (uint256[] memory values, int16 number) =
            choose ? (first, int8(-1)) : (second, int16(256));
        return (values[0], number);
    }

    function sliceChoice(bool choose) external view returns (uint256, int16) {
        return this.slicePair(choose, hex"0708", hex"090a");
    }

    function slicePair(bool choose, bytes calldata first, bytes calldata second)
        external pure returns (uint256, int16)
    {
        (bytes calldata values, int16 number) =
            choose ? (first, int8(-1)) : (second, int16(256));
        return (uint8(values[0]), number);
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
