//@ codegen-matrix: standard
//@ run-call: mix 0, 0, 42, 9 => 42, 12
//@ run-call: mix 5, 0x1000000000000000000000000000000000000000000000000, 17, 2 => 17, 510
//@ run-call: mix 8, 0, 99, 9 => 99, 1
//@ run-call: mix 7, 0x1f000000000000000000000000000000000000000000000000, 123, 1 => 123, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
contract EntryLastUse {
    function mix(uint256 seed, uint256 sample, uint256 retained, uint256 word)
        external pure returns (uint256, uint256)
    {
        uint256 result;
        unchecked {
            if ((seed & 8) == 0) {
                uint256 offset = 3 - (seed & 7);
                uint256 shift = ((sample >> 192) & 31) << 3;
                result = (word << shift) + offset;
            } else {
                result = seed ^ word;
            }
        }
        return (retained, result);
    }
}
