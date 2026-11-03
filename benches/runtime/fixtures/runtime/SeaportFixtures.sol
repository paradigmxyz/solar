// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/// @dev Local Seaport gas fixtures: every compiler's Seaport runs against this pinned code, so
/// token execution is identical. Their setters are open to anyone, so the benchmark resets the
/// state it reads before each compiler's calls.

/// @dev Answers the one call Seaport's constructor makes to its conduit controller. The orders
/// use no conduit, so nothing else is needed.
contract RuntimeSeaportConduitController {
    function getConduitCodeHashes() external pure returns (bytes32, bytes32) {
        return (keccak256("conduit creation code"), keccak256("conduit runtime code"));
    }
}

contract RuntimeSeaportToken {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function setBalance(address account, uint256 amount) external {
        balanceOf[account] = amount;
    }

    function setAllowance(address owner, address spender, uint256 amount) external {
        allowance[owner][spender] = amount;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        if (allowance[from][msg.sender] != type(uint256).max) {
            allowance[from][msg.sender] -= amount;
        }
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract RuntimeSeaportNFT {
    mapping(uint256 => address) public ownerOf;
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    function setOwner(uint256 tokenId, address owner) external {
        ownerOf[tokenId] = owner;
    }

    function setApproval(address owner, address operator, bool approved) external {
        isApprovedForAll[owner][operator] = approved;
    }

    function transferFrom(address from, address to, uint256 tokenId) external {
        require(ownerOf[tokenId] == from, "WRONG_FROM");
        require(msg.sender == from || isApprovedForAll[from][msg.sender], "NOT_AUTHORIZED");
        ownerOf[tokenId] = to;
    }
}
