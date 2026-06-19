// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ERC20.sol";

interface Vm {
    function prank(address) external;
    function expectEmit(bool, bool, bool, bool) external;
    function expectRevert(bytes calldata) external;
}

contract ERC20Test {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    ERC20 token;
    
    address alice = address(0x1);
    address bob = address(0x2);
    
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    
    function setUp() public {
        token = new ERC20("Test Token", "TST", 18);
    }
    
    function testMetadata() public view {
        assert(keccak256(bytes(token.name())) == keccak256("Test Token"));
        assert(keccak256(bytes(token.symbol())) == keccak256("TST"));
        assert(token.decimals() == 18);
    }
    
    function testMint() public {
        token.mint(alice, 1000);
        assert(token.balanceOf(alice) == 1000);
        assert(token.totalSupply() == 1000);
    }
    
    function testMintEmitsTransfer() public {
        vm.expectEmit(true, true, false, true);
        emit Transfer(address(0), alice, 500);
        token.mint(alice, 500);
    }
    
    function testTransfer() public {
        token.mint(alice, 1000);
        
        vm.prank(alice);
        token.transfer(bob, 300);
        
        assert(token.balanceOf(alice) == 700);
        assert(token.balanceOf(bob) == 300);
    }
    
    function testTransferEmitsEvent() public {
        token.mint(alice, 1000);
        
        vm.expectEmit(true, true, false, true);
        emit Transfer(alice, bob, 200);
        
        vm.prank(alice);
        token.transfer(bob, 200);
    }
    
    function testApprove() public {
        vm.prank(alice);
        token.approve(bob, 500);
        
        assert(token.allowance(alice, bob) == 500);
    }
    
    function testApproveEmitsEvent() public {
        vm.expectEmit(true, true, false, true);
        emit Approval(alice, bob, 500);
        
        vm.prank(alice);
        token.approve(bob, 500);
    }
    
    function testTransferFrom() public {
        token.mint(alice, 1000);
        
        vm.prank(alice);
        token.approve(bob, 600);
        
        vm.prank(bob);
        token.transferFrom(alice, bob, 400);
        
        assert(token.balanceOf(alice) == 600);
        assert(token.balanceOf(bob) == 400);
        assert(token.allowance(alice, bob) == 200);
    }
    
    function testBurn() public {
        token.mint(alice, 1000);
        
        vm.prank(alice);
        token.burn(300);
        
        assert(token.balanceOf(alice) == 700);
        assert(token.totalSupply() == 700);
    }
    
    function testBurnEmitsTransfer() public {
        token.mint(alice, 1000);
        
        vm.expectEmit(true, true, false, true);
        emit Transfer(alice, address(0), 200);
        
        vm.prank(alice);
        token.burn(200);
    }
    
    function testMultipleTransfers() public {
        token.mint(alice, 1000);
        
        vm.prank(alice);
        token.transfer(bob, 100);
        
        vm.prank(alice);
        token.transfer(bob, 200);
        
        vm.prank(bob);
        token.transfer(alice, 50);
        
        assert(token.balanceOf(alice) == 750);
        assert(token.balanceOf(bob) == 250);
    }
}
