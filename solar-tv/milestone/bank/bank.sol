contract Bank {
    mapping(address => uint256) balances;
    uint256 total;

    event Deposit(address indexed who, uint256 amount);

    function credit(address who, uint256 amount) internal returns (uint256) {
        balances[who] += amount;
        total += amount;
        return balances[who];
    }

    function deposit(uint256 amount) external returns (uint256) {
        uint256 b = credit(msg.sender, amount);
        emit Deposit(msg.sender, amount);
        return b;
    }

    function depositFor(address who, uint256 amount) external returns (uint256) {
        return credit(who, amount);
    }

    function balanceOf(address who) external view returns (uint256) {
        return balances[who];
    }
}
