// Reads after a join forward from a store both branches leave in place.
contract Joins {
    uint256 public total;

    function pick(bool flag, uint256 a) external returns (uint256) {
        total = a;
        uint256 b = flag ? a + 1 : a;
        return total + b;
    }
}
