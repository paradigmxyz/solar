//@ revisions: byzantium cancun
//@[byzantium] compile-flags: --evm-version byzantium
//@[cancun] compile-flags: --evm-version cancun
//@ run-call: PreConstantinopleShifts::shift(uint256,uint256) 3, 5 => 96, 0
//@ run-call: PreConstantinopleShifts::signedShift(int256,uint256) -3, 1 => -2
//@ run-call: PreConstantinopleShifts::signedShift(int256,uint256) -3, 300 => -1
//@ run-call: PreConstantinopleShifts::packedStorage() => 1193046
//@ run-call: PreConstantinopleShifts::partialCopy() => 0xab01020304050600000000000000000000000000000000000000000000000000

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
}
