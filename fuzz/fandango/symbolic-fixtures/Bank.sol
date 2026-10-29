// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Child {
    uint256 public v;

    constructor(uint256 x) {
        v = x;
    }
}

contract Bank {
    uint256 public immutable fee;
    mapping(address => uint256) public balance;
    uint256 public count;
    address public lastChild;

    event Deposit(address indexed who, uint256 amount);

    constructor(uint256 f) {
        require(f < 100);
        fee = f;
    }

    function deposit() external payable {
        balance[msg.sender] += msg.value;
        count += 1;
        emit Deposit(msg.sender, msg.value);
    }

    function withdraw(uint256 amount) external {
        balance[msg.sender] -= amount;
        uint256 out = amount - amount * fee / 100;
        (bool ok,) = msg.sender.call{value: out}("");
        require(ok);
    }

    function spawn(uint256 x) external returns (address c) {
        c = address(new Child(x));
        lastChild = c;
    }

    function quote(uint256 x) external view returns (uint256) {
        return x * fee / 100;
    }

    fallback() external payable {
        count += 2;
    }
}
