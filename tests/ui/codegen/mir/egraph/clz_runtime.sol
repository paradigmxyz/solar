//@ codegen-matrix: standard
//@ compile-flags: --evm-version osaka
//@ run-call: simplify 0 => 0, 0, 1, 1, 0, 256, 1, 0, 256, 0
//@ run-call: simplify 1 => 0, 1, 0, 0, 0, 255, 0, 0, 255, 0
//@ run-call: simplify 0x8000000000000000000000000000000000000000000000000000000000000000 => 1, 0, 0, 0, 0, 0, 0, 0, 0, 0

//@ run-call: inequalities 0 => 1, 0, 1, 1
//@ run-call: inequalities 1 => 0, 1, 1, 1
//@ run-call: inequalities 2 => 1, 1, 1, 1
//@ run-call: inequalities 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1, 1, 1, 1

contract YulClzSimplify {
    function inequalities(uint256 value) external pure returns (
        uint256 notOne, uint256 nonzero, uint256 bounded, uint256 boundedSwapped
    ) {
        assembly {
            notOne := iszero(eq(clz(value), 255))
            nonzero := iszero(eq(256, clz(value)))
            bounded := iszero(eq(clz(value), 257))
            boundedSwapped := iszero(eq(257, clz(value)))
        }
    }

    function simplify(uint256 value)
        external
        pure
        returns (
            uint256 isNegative,
            uint256 isOne,
            uint256 isZero,
            uint256 highByte,
            uint256 shiftedOut,
            uint256 masked,
            uint256 nextByte,
            uint256 quotient,
            uint256 remainder,
            uint256 knownNegative
        )
    {
        assembly {
            isNegative := iszero(clz(value))
            isOne := eq(clz(value), 255)
            isZero := eq(clz(value), 256)
            highByte := shr(8, clz(value))
            shiftedOut := shr(9, clz(value))
            masked := and(clz(value), 511)
            nextByte := byte(30, clz(value))
            quotient := div(clz(value), 257)
            remainder := mod(clz(value), 257)
            knownNegative :=
                clz(or(value, 0x8000000000000000000000000000000000000000000000000000000000000000))
        }
    }
}
