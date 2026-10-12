//@ codegen-matrix: standard
//@ run-call: shiftOut => 0x000000000000000000000000000000000000000000000000000000000000ff00

// Exact literal arithmetic folds `B << 8` to a value wider than the EVM word, which lowering must
// not materialize.
contract FixedBytesConstantWideShift {
    bytes32 constant B = 0xff000000000000000000000000000000000000000000000000000000000000ff;
    bytes32 constant SHIFT_OUT = B << 8;

    function shiftOut() external pure returns (bytes32) {
        return SHIFT_OUT;
    }
}
