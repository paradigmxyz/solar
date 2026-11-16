//@ codegen-matrix: standard
//@ run-call: wideShr() => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: mixedSub() => 292
//@ run-call: mixedShl() => 2
//@ run-call: mixedPow() => 256
//@ run-call: mixedAdd() => 264
//@ run-call: mixedU16() => 65535
// A literal paired with a typed operand must have a mobile type, but it does
// not have to fit the typed operand's own type: `256 + S8` with a `uint8` `S8`
// is performed in `uint16`. Two literals keep full precision, so the 257-bit
// intermediate of `(1 << 256) >> 1` is still folded. solc 0.8.36 accepts all
// of these and returns the same values.
contract C {
    uint256 constant ONE = 1;
    uint8 constant S8 = 8;
    uint16 constant U16 = 1;

    uint256 constant WIDE_SHR = (1 << 256) >> 1;
    uint256 constant MIXED_SUB = 300 - S8;
    uint256 constant MIXED_SHL = 1 << ONE;
    uint256 constant MIXED_POW = 2 ** S8;
    uint256 constant MIXED_ADD = 256 + S8;
    uint16 constant MIXED_U16 = 65534 + U16;

    function wideShr() public pure returns (uint256) {
        return WIDE_SHR;
    }

    function mixedSub() public pure returns (uint256) {
        return MIXED_SUB;
    }

    function mixedShl() public pure returns (uint256) {
        return MIXED_SHL;
    }

    function mixedPow() public pure returns (uint256) {
        return MIXED_POW;
    }

    function mixedAdd() public pure returns (uint256) {
        return MIXED_ADD;
    }

    function mixedU16() public pure returns (uint16) {
        return MIXED_U16;
    }
}
