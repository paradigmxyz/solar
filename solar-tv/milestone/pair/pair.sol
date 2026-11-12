contract Pair {
    uint256 total;

    function split(uint256 a) internal returns (uint256, uint256) {
        uint256 t = total + a;
        total = t;
        return (t, a ^ t);
    }

    function add(uint256 a) external returns (uint256) {
        (uint256 t, uint256 m) = split(a);
        return t ^ m;
    }

    function addTwice(uint256 a) external returns (uint256) {
        (uint256 t, uint256 m) = split(a);
        (uint256 u, uint256 n) = split(m);
        return t ^ u ^ n;
    }
}
