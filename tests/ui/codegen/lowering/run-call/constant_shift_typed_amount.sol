//@ codegen-matrix: standard
//@ run-call: shl255() => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: round255() => 1
//@ run-call: shl256() => 0
//@ run-call: round256() => 0
//@ run-call: shr256() => 0
//@ run-call: negativeShl() => -256
//@ run-call: negativePow() => 256
//@ run-call: pow() => 6561
//@ run-call-fail: pow256() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
// A shift or exponentiation whose left operand is a literal and whose right
// operand is typed is performed in `uint256` for a non-negative literal and in
// `int256` for a negative one, so an intermediate that leaves that range is
// not a constant. The initializer is then lowered as ordinary code and gets
// the EVM's behavior: shifting a bit out yields zero, and an exponent the
// result cannot hold reverts with `Panic(0x11)`. Folding at full precision
// instead returned 1 from `round256`.
contract C {
    uint16 constant S255 = 255;
    uint16 constant S256 = 256;
    uint8 constant S8 = 8;

    uint256 constant SHL255 = 1 << S255;
    uint256 constant ROUND255 = (1 << S255) >> S255;
    uint256 constant SHL256 = 1 << S256;
    uint256 constant ROUND256 = (1 << S256) >> S256;
    uint256 constant SHR256 = 1 >> S256;
    int256 constant NEG_SHL = (-1) << S8;
    int256 constant NEG_POW = (-2) ** S8;
    uint256 constant POW = 3 ** S8;
    uint256 constant POW256 = 2 ** S256;

    function shl255() public pure returns (uint256) {
        return SHL255;
    }

    function round255() public pure returns (uint256) {
        return ROUND255;
    }

    function shl256() public pure returns (uint256) {
        return SHL256;
    }

    function round256() public pure returns (uint256) {
        return ROUND256;
    }

    function shr256() public pure returns (uint256) {
        return SHR256;
    }

    function negativeShl() public pure returns (int256) {
        return NEG_SHL;
    }

    function negativePow() public pure returns (int256) {
        return NEG_POW;
    }

    function pow() public pure returns (uint256) {
        return POW;
    }

    function pow256() public pure returns (uint256) {
        return POW256;
    }
}
