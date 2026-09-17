//@ codegen-matrix: standard byzantium cancun
//@[byzantium] compile-flags: --evm-version byzantium
//@[cancun] compile-flags: --evm-version cancun
//@ run-call: PreConstantinopleShifts::shift 3, 5 => 96, 0
//@ run-call: PreConstantinopleShifts::signedShift -3, 1 => -2
//@ run-call: PreConstantinopleShifts::signedShift -3, 300 => -1
//@ run-call: PreConstantinopleShifts::packedStorage => 1193046
//@ run-call: PreConstantinopleShifts::partialCopy => 0xab01020304050600000000000000000000000000000000000000000000000000

//@ run-call: PreConstantinopleShifts::nested 1024 => 262144, 4, 4
//@ run-call: PreConstantinopleShifts::nested -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00, 0x00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, -1
//@ run-call: PreConstantinopleShifts::saturated -1 => 0, 0, -1
//@ run-call: PreConstantinopleShifts::saturated 42 => 0, 0, 0

contract PreConstantinopleShifts {
    uint24 internal packed;

    function shift(uint256 value, uint256 amount)
        external
        pure
        returns (uint256 left, uint256 right)
    {
        return (value << amount, value >> amount);
    }

    function signedShift(int256 value, uint256 amount) external pure returns (int256) {
        return value >> amount;
    }

    function packedStorage() external returns (uint256) {
        packed = 0x123456;
        return packed;
    }

    function partialCopy() external pure returns (bytes32 out) {
        bytes memory source = hex"ab010203040506";
        bytes memory copy = abi.decode(abi.encode(source), (bytes));
        assembly {
            out := mload(add(copy, 0x20))
        }
    }

    function nested(int256 value) external pure returns (uint256 left, uint256 right, int256 signed) {
        return ((uint256(value) << 3) << 5, (uint256(value) >> 3) >> 5, (value >> 3) >> 5);
    }

    function saturated(int256 value) external pure returns (uint256 left, uint256 right, int256 signed) {
        return (
            (uint256(value) << 128) << 128,
            (uint256(value) >> 255) >> 1,
            (value >> 1) >> type(uint256).max
        );
    }

}
