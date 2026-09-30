//@ revisions: none gas
//@[none] compile-flags: -O none -Zdump=secir
//@[gas] compile-flags: -O gas -Zdump=secir

// Security facts are computed from built MIR, so they do not depend on the optimization mode.
// Covers mapping keys, access-control guards, unchecked arithmetic, storage writes after an
// external call, and effects reached through an internal call.

contract Vault {
    address public owner;
    mapping(address => uint256) public balances;
    uint256 public total;

    constructor() {
        owner = msg.sender;
    }

    function deposit() external payable {
        balances[msg.sender] += msg.value;
        total += msg.value;
    }

    function withdraw(uint256 amount) external {
        require(balances[msg.sender] >= amount, "low");
        (bool ok,) = msg.sender.call{value: amount}("");
        require(ok);
        balances[msg.sender] -= amount;
        unchecked {
            total -= amount;
        }
    }

    function sweep(address to) external {
        require(msg.sender == owner);
        _send(to, address(this).balance);
        _clear();
    }

    function _send(address to, uint256 value) internal {
        payable(to).transfer(value);
    }

    function _clear() internal {
        total = 0;
    }
}
