//@ compile-flags: --evm-version osaka
//@ run-call: simplify 0 => 0, 0, 1, 1, 0, 256, 1, 0, 256, 0
//@ run-call: simplify 1 => 0, 1, 0, 0, 0, 255, 0, 0, 255, 0
//@ run-call: simplify 0x8000000000000000000000000000000000000000000000000000000000000000 => 1, 0, 0, 0, 0, 0, 0, 0, 0, 0

contract YulClzSimplify {
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
