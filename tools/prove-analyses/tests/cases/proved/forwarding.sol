// Reads CSE and load PRE forward from earlier stores, in storage and memory.
contract Forwarding {
    uint256 public total;
    mapping(address => uint256) public balances;

    function bump() external returns (uint256) {
        total = total + 1;
        return total;
    }

    function credit(uint256 amount) external returns (uint256) {
        balances[msg.sender] += amount;
        return balances[msg.sender];
    }

    function pair(uint256 a, uint256 b) external pure returns (uint256) {
        uint256[2] memory values = [a, b];
        return values[0] ^ values[1];
    }
}
