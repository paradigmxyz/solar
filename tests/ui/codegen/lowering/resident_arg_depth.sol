//@ compile-flags: -Ogas
//@ run-call: first 1 => 171
//@ run-call: second 2 => 189

contract ResidentArgDepth {
    function first(uint256 value) external pure returns (uint256) {
        return sum(value);
    }

    function second(uint256 value) external pure returns (uint256) {
        return sum(value);
    }

    function sum(uint256 value) internal pure returns (uint256) {
        unchecked {
            uint256 a0 = value + 1;
            uint256 a1 = value + 2;
            uint256 a2 = value + 3;
            uint256 a3 = value + 4;
            uint256 a4 = value + 5;
            uint256 a5 = value + 6;
            uint256 a6 = value + 7;
            uint256 a7 = value + 8;
            uint256 a8 = value + 9;
            uint256 a9 = value + 10;
            uint256 a10 = value + 11;
            uint256 a11 = value + 12;
            uint256 a12 = value + 13;
            uint256 a13 = value + 14;
            uint256 a14 = value + 15;
            uint256 a15 = value + 16;
            uint256 a16 = value + 17;
            return value + a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10
                + a11 + a12 + a13 + a14 + a15 + a16;
        }
    }
}
