contract Vault {
    mapping(address => uint256) balances;

    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }

    function balanceOf(address who) external view returns (uint256) {
        return balances[who];
    }
}
