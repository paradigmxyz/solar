contract DebugEgraphOrigins {
    function fold(uint256 x) external pure returns (uint256) {
        unchecked {
            return x + 1 + 2;
        }
    }

    function inverted(uint256 x, uint256 y) external pure returns (uint256) {
        if (!(x > y)) {
            return 1;
        }
        return 2;
    }

    function both(uint256 x, uint256 y) external pure returns (uint256 r) {
        unchecked {
            if (!(x + 1 + 2 > y)) {
                r = x * 7 + 1 + 2;
            }
        }
    }
}
