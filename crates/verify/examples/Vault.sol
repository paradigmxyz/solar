// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

/// Checks-effects-interactions done right: state is consistent before every external call,
/// so the invariant survives arbitrary reentrancy.
/// @custom:ghost uint256 sumShares
/// @custom:invariant totalShares == sumShares
/// @custom:invariant totalShares <= totalAssets
/// @custom:reentrancy-guard locked
contract Vault {
    IERC20 public immutable asset;
    uint256 public totalShares;
    uint256 public totalAssets;
    mapping(address => uint256) public shares;
    bool private locked;

    modifier nonReentrant() {
        require(!locked);
        locked = true;
        _;
        locked = false;
    }

    constructor(IERC20 _asset) {
        asset = _asset;
    }

    /// @custom:ensures shares[msg.sender] == old(shares[msg.sender]) + amount
    /// @custom:ensures !locked
    function deposit(uint256 amount) external nonReentrant {
        // Interaction first: anything the token does (including reentering us) happens
        // before we read or write our own state.
        require(asset.transferFrom(msg.sender, address(this), amount));
        totalAssets += amount;
        totalShares += amount;
        /// @custom:ghost sumShares = sumShares + amount
        shares[msg.sender] += amount;
    }

    /// @custom:ensures !locked
    function withdraw(uint256 amount) external nonReentrant {
        require(shares[msg.sender] >= amount);
        shares[msg.sender] -= amount;
        /// @custom:ghost sumShares = sumShares - amount
        totalShares -= amount;
        totalAssets -= amount;
        /// @custom:assert totalShares == sumShares && totalShares <= totalAssets
        require(asset.transfer(msg.sender, amount));
    }
}
