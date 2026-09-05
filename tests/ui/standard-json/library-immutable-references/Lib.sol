library Lib {
    function bump(uint256[] storage a, uint256 v) external returns (uint256) {
        a.push(v);
        return a.length;
    }
}

library ViewOnly {
    function peek(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }
}
