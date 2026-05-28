// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {ERC20} from "solmate/tokens/ERC20.sol";
import {ReentrancyGuard} from "solmate/utils/ReentrancyGuard.sol";

contract MultiInherit is ERC20, ReentrancyGuard {
    constructor() ERC20("Test", "TST", 18) {}
    
    function safeMint(address to, uint256 amount) external nonReentrant {
        _mint(to, amount);
    }
    
    function safeTransfer(address to, uint256 amount) external nonReentrant returns (bool) {
        return transfer(to, amount);
    }
}

contract MultiInheritTest is Test {
    MultiInherit token;
    
    function setUp() public {
        token = new MultiInherit();
    }
    
    function testSafeMint() public {
        token.safeMint(address(this), 1000);
        assertEq(token.balanceOf(address(this)), 1000);
    }
    
    function testSafeTransfer() public {
        token.safeMint(address(this), 1000);
        token.safeTransfer(address(1), 500);
        assertEq(token.balanceOf(address(1)), 500);
    }
}
