// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IERC20 {
    function balanceOf(address) external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
    function transferFrom(address, address, uint256) external returns (bool);
}

contract Vault {
    IERC20 public immutable asset;
    
    string public name;
    string public symbol;
    uint8 public decimals;
    uint256 public totalSupply;
    
    mapping(address => uint256) public balanceOf;
    
    event Deposit(address indexed caller, address indexed owner, uint256 assets, uint256 shares);
    event Withdraw(address indexed caller, address indexed receiver, address indexed owner, uint256 assets, uint256 shares);
    event Transfer(address indexed from, address indexed to, uint256 value);
    
    constructor(address _asset, string memory _name, string memory _symbol) {
        asset = IERC20(_asset);
        name = _name;
        symbol = _symbol;
        decimals = 18;
    }
    
    function totalAssets() public view returns (uint256) {
        return asset.balanceOf(address(this));
    }
    
    function convertToShares(uint256 assets) public view returns (uint256) {
        uint256 supply = totalSupply;
        if (supply == 0) {
            return assets;
        }
        return (assets * supply) / totalAssets();
    }
    
    function convertToAssets(uint256 shares) public view returns (uint256) {
        uint256 supply = totalSupply;
        if (supply == 0) {
            return shares;
        }
        return (shares * totalAssets()) / supply;
    }
    
    function deposit(uint256 assets, address receiver) external returns (uint256 shares) {
        shares = convertToShares(assets);
        require(shares > 0, "zero shares");
        
        asset.transferFrom(msg.sender, address(this), assets);
        
        totalSupply += shares;
        balanceOf[receiver] += shares;
        
        emit Deposit(msg.sender, receiver, assets, shares);
        emit Transfer(address(0), receiver, shares);
    }
    
    function withdraw(uint256 assets, address receiver, address owner) external returns (uint256 shares) {
        shares = convertToShares(assets);
        
        require(balanceOf[owner] >= shares, "insufficient shares");
        
        if (msg.sender != owner) {
            revert("not owner");
        }
        
        balanceOf[owner] -= shares;
        totalSupply -= shares;
        
        asset.transfer(receiver, assets);
        
        emit Withdraw(msg.sender, receiver, owner, assets, shares);
        emit Transfer(owner, address(0), shares);
    }
    
    function redeem(uint256 shares, address receiver, address owner) external returns (uint256 assets) {
        require(balanceOf[owner] >= shares, "insufficient shares");
        
        if (msg.sender != owner) {
            revert("not owner");
        }
        
        assets = convertToAssets(shares);
        
        balanceOf[owner] -= shares;
        totalSupply -= shares;
        
        asset.transfer(receiver, assets);
        
        emit Withdraw(msg.sender, receiver, owner, assets, shares);
        emit Transfer(owner, address(0), shares);
    }
    
    function maxDeposit(address) external pure returns (uint256) {
        return type(uint256).max;
    }
    
    function maxWithdraw(address owner) external view returns (uint256) {
        return convertToAssets(balanceOf[owner]);
    }
    
    function previewDeposit(uint256 assets) external view returns (uint256) {
        return convertToShares(assets);
    }
    
    function previewWithdraw(uint256 assets) external view returns (uint256) {
        return convertToShares(assets);
    }
}
