// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for multiple stacked modifiers
/// @notice Tests compiler handling of complex modifier patterns

contract StressModifiers {
    address public owner;
    address public admin;
    bool public paused;
    bool public locked;
    uint256 public value;
    uint256 public callCount;
    uint256 public beforeCount;
    uint256 public afterCount;
    
    mapping(address => bool) public whitelist;
    mapping(address => uint256) public balances;
    mapping(bytes32 => bool) public roles;
    
    constructor() {
        owner = msg.sender;
        admin = msg.sender;
    }
    
    // ========== Basic modifiers ==========
    
    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }
    
    modifier onlyAdmin() {
        require(msg.sender == admin, "Not admin");
        _;
    }
    
    modifier onlyOwnerOrAdmin() {
        require(msg.sender == owner || msg.sender == admin, "Not authorized");
        _;
    }
    
    // ========== State check modifiers ==========
    
    modifier whenNotPaused() {
        require(!paused, "Paused");
        _;
    }
    
    modifier whenPaused() {
        require(paused, "Not paused");
        _;
    }
    
    modifier nonReentrant() {
        require(!locked, "Reentrant");
        locked = true;
        _;
        locked = false;
    }
    
    // ========== Parameter validation modifiers ==========
    
    modifier validValue(uint256 v) {
        require(v > 0, "Invalid value");
        _;
    }
    
    modifier validAddress(address addr) {
        require(addr != address(0), "Zero address");
        _;
    }
    
    modifier validRange(uint256 v, uint256 min, uint256 max) {
        require(v >= min && v <= max, "Out of range");
        _;
    }
    
    modifier maxValue(uint256 v, uint256 max) {
        require(v <= max, "Exceeds max");
        _;
    }
    
    modifier minValue(uint256 v, uint256 min) {
        require(v >= min, "Below min");
        _;
    }
    
    // ========== Counting modifiers (for testing execution order) ==========
    
    modifier countBefore() {
        beforeCount++;
        _;
    }
    
    modifier countAfter() {
        _;
        afterCount++;
    }
    
    modifier countBoth() {
        beforeCount++;
        _;
        afterCount++;
    }
    
    modifier countCall() {
        callCount++;
        _;
    }
    
    // ========== Whitelist modifiers ==========
    
    modifier onlyWhitelisted() {
        require(whitelist[msg.sender], "Not whitelisted");
        _;
    }
    
    modifier onlyWhitelistedOrOwner() {
        require(whitelist[msg.sender] || msg.sender == owner, "Not authorized");
        _;
    }
    
    // ========== Role-based modifiers ==========
    
    modifier hasRole(bytes32 role) {
        require(roles[role], "Missing role");
        _;
    }
    
    // ========== Balance modifiers ==========
    
    modifier hasBalance(uint256 amount) {
        require(balances[msg.sender] >= amount, "Insufficient balance");
        _;
    }
    
    // ========== Admin functions ==========
    
    function setOwner(address newOwner) public virtual onlyOwner validAddress(newOwner) {
        owner = newOwner;
    }
    
    function setAdmin(address newAdmin) public onlyOwner validAddress(newAdmin) {
        admin = newAdmin;
    }
    
    function pause() public onlyOwnerOrAdmin whenNotPaused {
        paused = true;
    }
    
    function unpause() public onlyOwnerOrAdmin whenPaused {
        paused = false;
    }
    
    function addToWhitelist(address addr) public onlyOwner validAddress(addr) {
        whitelist[addr] = true;
    }
    
    function removeFromWhitelist(address addr) public onlyOwner {
        whitelist[addr] = false;
    }
    
    function setRole(bytes32 role, bool enabled) public onlyOwner {
        roles[role] = enabled;
    }
    
    function deposit() public payable whenNotPaused {
        balances[msg.sender] += msg.value;
    }
    
    // ========== Single modifier functions ==========
    
    function singleModifier1() public onlyOwner {
        value = 1;
    }
    
    function singleModifier2() public whenNotPaused {
        value = 2;
    }
    
    function singleModifier3() public nonReentrant {
        value = 3;
    }
    
    // ========== Two stacked modifiers ==========
    
    function twoModifiers1() public onlyOwner whenNotPaused {
        value = 10;
    }
    
    function twoModifiers2() public onlyOwner nonReentrant {
        value = 11;
    }
    
    function twoModifiers3() public whenNotPaused nonReentrant {
        value = 12;
    }
    
    function twoModifiers4() public onlyOwner validValue(1) {
        value = 13;
    }
    
    // ========== Three stacked modifiers ==========
    
    function threeModifiers1() public onlyOwner whenNotPaused nonReentrant {
        value = 20;
    }
    
    function threeModifiers2() public onlyOwner whenNotPaused validValue(1) {
        value = 21;
    }
    
    function threeModifiers3() public onlyOwner nonReentrant countCall() {
        value = 22;
    }
    
    // ========== Four stacked modifiers ==========
    
    function fourModifiers1() public onlyOwner whenNotPaused nonReentrant validValue(1) {
        value = 30;
    }
    
    function fourModifiers2() public onlyOwner whenNotPaused nonReentrant countCall() {
        value = 31;
    }
    
    // ========== Five stacked modifiers ==========
    
    function fiveModifiers1() public onlyOwner whenNotPaused nonReentrant validValue(1) countCall() {
        value = 40;
    }
    
    function fiveModifiers2() public onlyOwner whenNotPaused nonReentrant countBefore() countAfter() {
        value = 41;
    }
    
    // ========== Six stacked modifiers (extreme case) ==========
    
    function sixModifiers() public onlyOwner whenNotPaused nonReentrant validValue(1) countBefore() countAfter() {
        value = 50;
    }
    
    // ========== Modifiers with parameters ==========
    
    function withParamModifiers1(uint256 v) public validValue(v) validRange(v, 1, 100) {
        value = v;
    }
    
    function withParamModifiers2(uint256 v) public onlyOwner validValue(v) maxValue(v, 1000) {
        value = v;
    }
    
    function withParamModifiers3(uint256 v) public onlyOwner whenNotPaused validRange(v, 10, 500) {
        value = v;
    }
    
    // ========== Counting modifiers for order testing ==========
    
    function testOrderBefore() public countBefore() countBefore() countBefore() {
        value = 100;
    }
    
    function testOrderAfter() public countAfter() countAfter() countAfter() {
        value = 101;
    }
    
    function testOrderBoth() public countBoth() countBoth() countBoth() {
        value = 102;
    }
    
    function testOrderMixed() public countBefore() countAfter() countBoth() {
        value = 103;
    }
    
    // ========== Reset functions ==========
    
    function resetCounts() public onlyOwner {
        callCount = 0;
        beforeCount = 0;
        afterCount = 0;
    }
    
    function resetValue() public onlyOwner {
        value = 0;
    }
    
    // ========== Getter functions ==========
    
    function getCounts() public view returns (uint256, uint256, uint256) {
        return (callCount, beforeCount, afterCount);
    }
    
    function isLocked() public view returns (bool) {
        return locked;
    }
}

/// @title Test inheritance with modifiers
contract ChildWithModifiers is StressModifiers {
    uint256 public childValue;
    
    modifier onlyPositive(uint256 v) {
        require(v > 0, "Not positive");
        _;
    }
    
    // Override parent modifier behavior
    function setOwner(address newOwner) public override onlyOwner validAddress(newOwner) {
        // Additional logic in child
        childValue = 1;
        owner = newOwner;
    }
    
    // New function with inherited + new modifiers
    function setChildValue(uint256 v) public onlyOwner whenNotPaused onlyPositive(v) {
        childValue = v;
    }
    
    // Combined parent and child modifiers
    function complexChild(uint256 v) public onlyOwner whenNotPaused nonReentrant onlyPositive(v) countCall() {
        childValue = v;
        value = v * 2;
    }
}

/// @title Test modifier reuse across contracts
abstract contract ModifierBase {
    bool internal _initialized;
    
    modifier initializer() {
        require(!_initialized, "Already initialized");
        _;
        _initialized = true;
    }
    
    modifier whenInitialized() {
        require(_initialized, "Not initialized");
        _;
    }
}

contract InitializableContract is ModifierBase {
    uint256 public data;
    address public initOwner;
    
    function initialize(uint256 _data) public initializer {
        data = _data;
        initOwner = msg.sender;
    }
    
    function updateData(uint256 _data) public whenInitialized {
        require(msg.sender == initOwner, "Not owner");
        data = _data;
    }
    
    function isInitialized() public view returns (bool) {
        return _initialized;
    }
}
