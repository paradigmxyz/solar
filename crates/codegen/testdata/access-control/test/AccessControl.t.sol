// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/AccessControl.sol";

interface Vm {
    function prank(address) external;
    function expectEmit(bool, bool, bool, bool) external;
    function expectRevert(bytes calldata) external;
}

contract AccessControlTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    AccessControl ac;
    
    address alice = address(0x1);
    address bob = address(0x2);
    address charlie = address(0x3);
    
    bytes32 constant ADMIN_ROLE = keccak256("ADMIN_ROLE");
    bytes32 constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    
    event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
    event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Paused(address account);
    event Unpaused(address account);
    
    function setUp() public {
        ac = new AccessControl();
    }
    
    function testOwnerIsDeployer() public view {
        assert(ac.owner() == address(this));
    }
    
    function testDeployerHasAdminRole() public view {
        assert(ac.hasRole(ADMIN_ROLE, address(this)));
    }
    
    function testGrantRole() public {
        ac.grantRole(MINTER_ROLE, alice);
        assert(ac.hasRole(MINTER_ROLE, alice));
    }
    
    function testGrantRoleEmitsEvent() public {
        vm.expectEmit(true, true, true, true);
        emit RoleGranted(MINTER_ROLE, alice, address(this));
        ac.grantRole(MINTER_ROLE, alice);
    }
    
    function testRevokeRole() public {
        ac.grantRole(MINTER_ROLE, alice);
        ac.revokeRole(MINTER_ROLE, alice);
        assert(!ac.hasRole(MINTER_ROLE, alice));
    }
    
    function testRevokeRoleEmitsEvent() public {
        ac.grantRole(MINTER_ROLE, alice);
        
        vm.expectEmit(true, true, true, true);
        emit RoleRevoked(MINTER_ROLE, alice, address(this));
        ac.revokeRole(MINTER_ROLE, alice);
    }
    
    function testRenounceRole() public {
        ac.grantRole(MINTER_ROLE, alice);
        
        vm.prank(alice);
        ac.renounceRole(MINTER_ROLE);
        
        assert(!ac.hasRole(MINTER_ROLE, alice));
    }
    
    function testTransferOwnership() public {
        ac.transferOwnership(alice);
        assert(ac.owner() == alice);
    }
    
    function testTransferOwnershipEmitsEvent() public {
        vm.expectEmit(true, true, false, true);
        emit OwnershipTransferred(address(this), alice);
        ac.transferOwnership(alice);
    }
    
    function testRenounceOwnership() public {
        ac.renounceOwnership();
        assert(ac.owner() == address(0));
    }
    
    function testPause() public {
        ac.grantRole(PAUSER_ROLE, alice);
        
        vm.prank(alice);
        ac.pause();
        
        assert(ac.paused());
    }
    
    function testPauseEmitsEvent() public {
        ac.grantRole(PAUSER_ROLE, alice);
        
        vm.expectEmit(false, false, false, true);
        emit Paused(alice);
        
        vm.prank(alice);
        ac.pause();
    }
    
    function testUnpause() public {
        ac.grantRole(PAUSER_ROLE, alice);
        
        vm.prank(alice);
        ac.pause();
        
        vm.prank(alice);
        ac.unpause();
        
        assert(!ac.paused());
    }
    
    function testSetValueWithMinterRole() public {
        ac.grantRole(MINTER_ROLE, alice);
        
        vm.prank(alice);
        ac.setValue(42);
        
        assert(ac.value() == 42);
    }
    
    function testAdminSetValueAsOwner() public {
        ac.adminSetValue(100);
        assert(ac.value() == 100);
    }
    
    function testPublicRead() public view {
        assert(ac.publicRead() == 0);
    }
    
    function testMultipleRoles() public {
        ac.grantRole(MINTER_ROLE, alice);
        ac.grantRole(PAUSER_ROLE, alice);
        ac.grantRole(ADMIN_ROLE, bob);
        
        assert(ac.hasRole(MINTER_ROLE, alice));
        assert(ac.hasRole(PAUSER_ROLE, alice));
        assert(ac.hasRole(ADMIN_ROLE, bob));
        assert(!ac.hasRole(MINTER_ROLE, bob));
    }
    
    function testRoleConstants() public view {
        assert(ac.ADMIN_ROLE() == keccak256("ADMIN_ROLE"));
        assert(ac.MINTER_ROLE() == keccak256("MINTER_ROLE"));
        assert(ac.PAUSER_ROLE() == keccak256("PAUSER_ROLE"));
    }
    
    function testChainedOwnership() public {
        ac.transferOwnership(alice);
        
        vm.prank(alice);
        ac.transferOwnership(bob);
        
        assert(ac.owner() == bob);
    }
    
    function testGrantRoleByNewAdmin() public {
        ac.grantRole(ADMIN_ROLE, alice);
        
        vm.prank(alice);
        ac.grantRole(MINTER_ROLE, bob);
        
        assert(ac.hasRole(MINTER_ROLE, bob));
    }
}
