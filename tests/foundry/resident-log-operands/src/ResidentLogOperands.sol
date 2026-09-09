// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ResidentLogOperands {
    mapping(address => mapping(address => uint256)) public allowance;
    mapping(address => uint256) public fees;
    uint256 public saved;

    event Approval(address indexed owner, address indexed spender, uint256 amount);
    event Fee(address indexed token, uint256 amount);
    event Repeated(address indexed first, address indexed second, uint256 amount);

    function approve(address owner, address spender, uint256 amount) external {
        allowance[owner][spender] = amount;
        emit Approval(owner, spender, amount);
    }

    function setFee(address token, uint256 amount) external {
        emit Fee(token, amount);
        fees[token] = amount;
    }

    function repeated(address actor, uint256 amount) external {
        emit Repeated(actor, actor, amount);
        saved = uint256(uint160(actor));
    }

    function retain(address owner, address spender, uint256 amount, uint256 keep)
        external returns (uint256)
    {
        emit Approval(owner, spender, amount);
        saved = keep;
        return keep;
    }

    function repeatedZero(address actor, uint256 amount) external {
        assembly {
            mstore(0, amount)
            log3(0, 32, 0, actor, actor)
        }
        saved = uint256(uint160(actor));
    }
}
