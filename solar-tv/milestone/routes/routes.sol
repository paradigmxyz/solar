contract Routes {
    error E(uint256 code);

    uint256 x;

    receive() external payable {
        x = 1;
    }

    fallback() external payable {
        x = 2;
    }

    function check(uint256 a) external pure {
        if (a > 10) revert E(a);
    }
}
