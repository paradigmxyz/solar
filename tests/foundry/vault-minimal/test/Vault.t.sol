// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Vault.sol";
import "../src/MockERC20.sol";

interface Vm {
    function prank(address) external;
    function expectEmit(bool, bool, bool, bool) external;
}

contract VaultTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    
    MockERC20 token;
    Vault vault;
    
    address alice = address(0x1);
    address bob = address(0x2);
    
    event Deposit(address indexed caller, address indexed owner, uint256 assets, uint256 shares);
    event Withdraw(address indexed caller, address indexed receiver, address indexed owner, uint256 assets, uint256 shares);
    
    function setUp() public {
        token = new MockERC20("Test Token", "TST", 18);
        vault = new Vault(address(token), "Vault Token", "vTST");
        
        token.mint(alice, 10000);
        token.mint(bob, 10000);
        
        vm.prank(alice);
        token.approve(address(vault), type(uint256).max);
        
        vm.prank(bob);
        token.approve(address(vault), type(uint256).max);
    }
    
    function testMetadata() public view {
        assert(keccak256(bytes(vault.name())) == keccak256("Vault Token"));
        assert(keccak256(bytes(vault.symbol())) == keccak256("vTST"));
        assert(vault.decimals() == 18);
    }
    
    function testInitialState() public view {
        assert(vault.totalSupply() == 0);
        assert(vault.totalAssets() == 0);
    }
    
    function testDeposit() public {
        vm.prank(alice);
        uint256 shares = vault.deposit(1000, alice);
        
        assert(shares == 1000);
        assert(vault.balanceOf(alice) == 1000);
        assert(vault.totalSupply() == 1000);
        assert(vault.totalAssets() == 1000);
        assert(token.balanceOf(alice) == 9000);
    }
    
    function testDepositEmitsEvents() public {
        vm.expectEmit(true, true, false, true);
        emit Deposit(alice, alice, 1000, 1000);
        
        vm.prank(alice);
        vault.deposit(1000, alice);
    }
    
    function testDepositForOther() public {
        vm.prank(alice);
        vault.deposit(1000, bob);
        
        assert(vault.balanceOf(alice) == 0);
        assert(vault.balanceOf(bob) == 1000);
    }
    
    function testWithdraw() public {
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        vm.prank(alice);
        vault.withdraw(500, alice, alice);
        
        assert(vault.balanceOf(alice) == 500);
        assert(vault.totalSupply() == 500);
        assert(vault.totalAssets() == 500);
        assert(token.balanceOf(alice) == 9500);
    }
    
    function testRedeem() public {
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        vm.prank(alice);
        uint256 assets = vault.redeem(500, alice, alice);
        
        assert(assets == 500);
        assert(vault.balanceOf(alice) == 500);
        assert(token.balanceOf(alice) == 9500);
    }
    
    function testConvertToShares() public {
        assert(vault.convertToShares(1000) == 1000);
        
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        assert(vault.convertToShares(500) == 500);
    }
    
    function testConvertToAssets() public {
        assert(vault.convertToAssets(1000) == 1000);
        
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        assert(vault.convertToAssets(500) == 500);
    }
    
    function testMultipleDeposits() public {
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        vm.prank(bob);
        vault.deposit(1000, bob);
        
        assert(vault.balanceOf(alice) == 1000);
        assert(vault.balanceOf(bob) == 1000);
        assert(vault.totalSupply() == 2000);
        assert(vault.totalAssets() == 2000);
    }
    
    function testMaxDeposit() public view {
        assert(vault.maxDeposit(alice) == type(uint256).max);
    }
    
    function testMaxWithdraw() public {
        vm.prank(alice);
        vault.deposit(1000, alice);
        
        assert(vault.maxWithdraw(alice) == 1000);
        assert(vault.maxWithdraw(bob) == 0);
    }
    
    function testPreviewDeposit() public view {
        assert(vault.previewDeposit(1000) == 1000);
    }
    
    function testDepositWithdrawCycle() public {
        vm.prank(alice);
        vault.deposit(2000, alice);
        
        vm.prank(alice);
        vault.redeem(500, alice, alice);
        
        uint256 remaining = vault.balanceOf(alice);
        vm.prank(alice);
        vault.redeem(remaining, alice, alice);
        
        assert(vault.balanceOf(alice) == 0);
        assert(vault.totalSupply() == 0);
    }
}
