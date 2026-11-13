contract DynArr {
    function pair(uint256 x) external pure returns (uint256[] memory a) {
        a = new uint256[](2);
        a[0] = x;
        a[1] = x ^ 7;
    }
}
