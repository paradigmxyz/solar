//@ codegen-matrix: standard
//@[none] compile-flags: --evm-version=byzantium
//@ run-call: mul32 0 => 0
//@ run-call: mul32 1 => 32
//@ run-call: mul32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call: mul32 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: mulHigh 3 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: mulHigh 2 => 0
//@ run-call: at [11,22,33], 0 => 11
//@ run-call: at [11,22,33], 2 => 33
//@ run-call-fail: at [11,22,33], 3 => Panic(0x32)

contract PowerOfTwoMul {
    function mul32(uint256 value) external pure returns (uint256 result) {
        assembly { result := mul(value, 32) }
    }

    function mulHigh(uint256 value) external pure returns (uint256 result) {
        assembly {
            result := mul(0x8000000000000000000000000000000000000000000000000000000000000000, value)
        }
    }

    function at(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }
}
