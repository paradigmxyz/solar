// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {ERC20} from "solmate/tokens/ERC20.sol";

contract MinimalERC20 is ERC20 {
    constructor() ERC20("Test", "TST", 18) {}
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract MinimalERC20Test is Test {
    MinimalERC20 token;
    
    function setUp() public {
        token = new MinimalERC20();
    }
    
    function testMint() public {
        token.mint(address(this), 1000);
        assertEq(token.balanceOf(address(this)), 1000);
    }
    
    function testTransfer() public {
        token.mint(address(this), 1000);
        token.transfer(address(1), 500);
        assertEq(token.balanceOf(address(1)), 500);
    }
}
