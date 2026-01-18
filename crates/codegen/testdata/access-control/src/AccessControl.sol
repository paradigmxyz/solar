// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AccessControl {
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    
    address public owner;
    bool public paused;
    uint256 public value;
    
    mapping(bytes32 => mapping(address => bool)) private _roles;
    
    event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
    event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Paused(address account);
    event Unpaused(address account);
    
    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }
    
    modifier onlyRole(bytes32 role) {
        require(hasRole(role, msg.sender), "missing role");
        _;
    }
    
    modifier whenNotPaused() {
        require(!paused, "paused");
        _;
    }
    
    modifier whenPaused() {
        require(paused, "not paused");
        _;
    }
    
    constructor() {
        owner = msg.sender;
        _roles[ADMIN_ROLE][msg.sender] = true;
        emit OwnershipTransferred(address(0), msg.sender);
        emit RoleGranted(ADMIN_ROLE, msg.sender, msg.sender);
    }
    
    function hasRole(bytes32 role, address account) public view returns (bool) {
        return _roles[role][account];
    }
    
    function grantRole(bytes32 role, address account) external onlyRole(ADMIN_ROLE) {
        if (!_roles[role][account]) {
            _roles[role][account] = true;
            emit RoleGranted(role, account, msg.sender);
        }
    }
    
    function revokeRole(bytes32 role, address account) external onlyRole(ADMIN_ROLE) {
        if (_roles[role][account]) {
            _roles[role][account] = false;
            emit RoleRevoked(role, account, msg.sender);
        }
    }
    
    function renounceRole(bytes32 role) external {
        require(_roles[role][msg.sender], "no role");
        _roles[role][msg.sender] = false;
        emit RoleRevoked(role, msg.sender, msg.sender);
    }
    
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "zero address");
        address oldOwner = owner;
        owner = newOwner;
        emit OwnershipTransferred(oldOwner, newOwner);
    }
    
    function renounceOwnership() external onlyOwner {
        address oldOwner = owner;
        owner = address(0);
        emit OwnershipTransferred(oldOwner, address(0));
    }
    
    function pause() external onlyRole(PAUSER_ROLE) whenNotPaused {
        paused = true;
        emit Paused(msg.sender);
    }
    
    function unpause() external onlyRole(PAUSER_ROLE) whenPaused {
        paused = false;
        emit Unpaused(msg.sender);
    }
    
    function setValue(uint256 _value) external onlyRole(MINTER_ROLE) whenNotPaused {
        value = _value;
    }
    
    function adminSetValue(uint256 _value) external onlyOwner {
        value = _value;
    }
    
    function publicRead() external view returns (uint256) {
        return value;
    }
}
