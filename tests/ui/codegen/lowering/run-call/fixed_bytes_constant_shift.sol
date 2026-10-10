//@ codegen-matrix: standard
//@ run-call: low => 0x00000000000000000000000000000000ffffffffffffffffffffffffffffffff
//@ run-call: high => 0xffffffffffffffffffffffffffffffff00000000000000000000000000000000
//@ run-call: kept => 0xf0
//@ run-call: dropped => 0x0f

// Shifts of `bytesN` constants drop the bits that leave the type, like at runtime.
contract FixedBytesConstantShift {
    bytes32 constant FULL = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
    bytes32 constant LOW = (FULL << 128) >> 128;
    bytes32 constant HIGH = (FULL >> 128) << 128;
    bytes1 constant BYTE = 0xff;
    bytes1 constant KEPT = (BYTE >> 4) << 4;
    bytes1 constant DROPPED = (BYTE << 4) >> 4;

    function low() external pure returns (bytes32) {
        return LOW;
    }

    function high() external pure returns (bytes32) {
        return HIGH;
    }

    function kept() external pure returns (bytes1) {
        return KEPT;
    }

    function dropped() external pure returns (bytes1) {
        return DROPPED;
    }
}
