// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ERC721.sol";

interface Vm {
    function prank(address) external;
    function expectEmit(bool, bool, bool, bool) external;
}

contract ERC721Test {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    ERC721 nft;
    
    address alice = address(0x1);
    address bob = address(0x2);
    address charlie = address(0x3);
    
    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);
    
    function setUp() public {
        nft = new ERC721("Test NFT", "TNFT");
    }
    
    function testMetadata() public view {
        assert(keccak256(bytes(nft.name())) == keccak256("Test NFT"));
        assert(keccak256(bytes(nft.symbol())) == keccak256("TNFT"));
    }
    
    function testMint() public {
        uint256 tokenId = nft.mint(alice);
        assert(tokenId == 0);
        assert(nft.ownerOf(0) == alice);
        assert(nft.balanceOf(alice) == 1);
    }
    
    function testMintMultiple() public {
        nft.mint(alice);
        nft.mint(alice);
        nft.mint(bob);
        
        assert(nft.balanceOf(alice) == 2);
        assert(nft.balanceOf(bob) == 1);
        assert(nft.ownerOf(0) == alice);
        assert(nft.ownerOf(1) == alice);
        assert(nft.ownerOf(2) == bob);
    }
    
    function testMintEmitsTransfer() public {
        vm.expectEmit(true, true, true, true);
        emit Transfer(address(0), alice, 0);
        nft.mint(alice);
    }
    
    function testTransferFrom() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.transferFrom(alice, bob, 0);
        
        assert(nft.ownerOf(0) == bob);
        assert(nft.balanceOf(alice) == 0);
        assert(nft.balanceOf(bob) == 1);
    }
    
    function testTransferFromEmitsEvent() public {
        nft.mint(alice);
        
        vm.expectEmit(true, true, true, true);
        emit Transfer(alice, bob, 0);
        
        vm.prank(alice);
        nft.transferFrom(alice, bob, 0);
    }
    
    function testApprove() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.approve(bob, 0);
        
        assert(nft.getApproved(0) == bob);
    }
    
    function testApproveEmitsEvent() public {
        nft.mint(alice);
        
        vm.expectEmit(true, true, true, true);
        emit Approval(alice, bob, 0);
        
        vm.prank(alice);
        nft.approve(bob, 0);
    }
    
    function testTransferFromApproved() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.approve(bob, 0);
        
        vm.prank(bob);
        nft.transferFrom(alice, charlie, 0);
        
        assert(nft.ownerOf(0) == charlie);
        assert(nft.getApproved(0) == address(0));
    }
    
    function testSetApprovalForAll() public {
        vm.prank(alice);
        nft.setApprovalForAll(bob, true);
        
        assert(nft.isApprovedForAll(alice, bob));
    }
    
    function testApprovalForAllEmitsEvent() public {
        vm.expectEmit(true, true, false, true);
        emit ApprovalForAll(alice, bob, true);
        
        vm.prank(alice);
        nft.setApprovalForAll(bob, true);
    }
    
    function testTransferFromOperator() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.setApprovalForAll(bob, true);
        
        vm.prank(bob);
        nft.transferFrom(alice, charlie, 0);
        
        assert(nft.ownerOf(0) == charlie);
    }
    
    function testBurn() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.burn(0);
        
        assert(nft.ownerOf(0) == address(0));
        assert(nft.balanceOf(alice) == 0);
    }
    
    function testBurnEmitsTransfer() public {
        nft.mint(alice);
        
        vm.expectEmit(true, true, true, true);
        emit Transfer(alice, address(0), 0);
        
        vm.prank(alice);
        nft.burn(0);
    }
    
    function testBurnClearsApproval() public {
        nft.mint(alice);
        
        vm.prank(alice);
        nft.approve(bob, 0);
        
        vm.prank(alice);
        nft.burn(0);
        
        assert(nft.getApproved(0) == address(0));
    }
}
