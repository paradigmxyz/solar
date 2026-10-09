//@ codegen-matrix: standard
//@ run-call: low => 0x00000000000000000000000000000000ffffffffffffffffffffffffffffffff
//@ run-call: high => 0xffffffffffffffffffffffffffffffff00000000000000000000000000000000
//@ run-call: kept => 0x0f

// Shifts of `bytesN` constants drop the bits that leave the type, like at runtime.
contract FixedBytesConstantShift {
    bytes32 constant FULL = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
    bytes32 constant LOW = (FULL << 128) >> 128;
    bytes32 constant HIGH = (FULL >> 128) << 128;
    bytes1 constant KEPT = (bytes1(0xff) >> 4) << 0;

    function low() external pure returns (bytes32) {
        return LOW;
    }

    function high() external pure returns (bytes32) {
        return HIGH;
    }

    function kept() external pure returns (bytes1) {
        return KEPT;
    }
}
