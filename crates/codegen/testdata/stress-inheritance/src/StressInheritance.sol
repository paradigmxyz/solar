// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for deep inheritance (6+ levels)
/// @notice Tests compiler handling of complex inheritance hierarchies

// ========== Linear Deep Inheritance Chain (8 levels) ==========

contract Level0 {
    uint256 public value0;
    function set0(uint256 v) public virtual { value0 = v; }
    function get0() public view returns (uint256) { return value0; }
}

contract Level1 is Level0 {
    uint256 public value1;
    function set1(uint256 v) public virtual { value1 = v; }
    function sum1() public view returns (uint256) { return value0 + value1; }
}

contract Level2 is Level1 {
    uint256 public value2;
    function set2(uint256 v) public virtual { value2 = v; }
    function sum2() public view returns (uint256) { return sum1() + value2; }
}

contract Level3 is Level2 {
    uint256 public value3;
    function set3(uint256 v) public virtual { value3 = v; }
    function sum3() public view returns (uint256) { return sum2() + value3; }
}

contract Level4 is Level3 {
    uint256 public value4;
    function set4(uint256 v) public virtual { value4 = v; }
    function sum4() public view returns (uint256) { return sum3() + value4; }
}

contract Level5 is Level4 {
    uint256 public value5;
    function set5(uint256 v) public virtual { value5 = v; }
    function sum5() public view returns (uint256) { return sum4() + value5; }
}

contract Level6 is Level5 {
    uint256 public value6;
    function set6(uint256 v) public virtual { value6 = v; }
    function sum6() public view returns (uint256) { return sum5() + value6; }
}

contract Level7 is Level6 {
    uint256 public value7;
    function set7(uint256 v) public virtual { value7 = v; }
    function sum7() public view returns (uint256) { return sum6() + value7; }
}

contract DeepLinear is Level7 {
    uint256 public value8;
    
    function set8(uint256 v) public { value8 = v; }
    
    function sumAll() public view returns (uint256) {
        return sum7() + value8;
    }
    
    function setAll(uint256 v) public {
        set0(v);
        set1(v);
        set2(v);
        set3(v);
        set4(v);
        set5(v);
        set6(v);
        set7(v);
        set8(v);
    }
    
    function getAllValues() public view returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256) {
        return (value0, value1, value2, value3, value4, value5, value6, value7, value8);
    }
}

// ========== Diamond Inheritance Pattern ==========

interface IValueHolder {
    function getValue() external view returns (uint256);
}

abstract contract BaseA {
    uint256 internal _valueA;
    
    function setValueA(uint256 v) public virtual {
        _valueA = v;
    }
    
    function getValueA() public view returns (uint256) {
        return _valueA;
    }
}

abstract contract BaseB {
    uint256 internal _valueB;
    
    function setValueB(uint256 v) public virtual {
        _valueB = v;
    }
    
    function getValueB() public view returns (uint256) {
        return _valueB;
    }
}

abstract contract MiddleAB is BaseA, BaseB {
    uint256 internal _valueAB;
    
    function setValueAB(uint256 v) public virtual {
        _valueAB = v;
    }
    
    function getValueAB() public view returns (uint256) {
        return _valueAB;
    }
    
    function sumAB() public view returns (uint256) {
        return _valueA + _valueB + _valueAB;
    }
}

abstract contract ExtendedA is MiddleAB {
    uint256 internal _extendedA;
    
    function setExtendedA(uint256 v) public virtual {
        _extendedA = v;
    }
    
    function getExtendedA() public view returns (uint256) {
        return _extendedA;
    }
    
    function setValueA(uint256 v) public virtual override {
        _valueA = v + 1; // Modified behavior
    }
}

abstract contract ExtendedB is MiddleAB {
    uint256 internal _extendedB;
    
    function setExtendedB(uint256 v) public virtual {
        _extendedB = v;
    }
    
    function getExtendedB() public view returns (uint256) {
        return _extendedB;
    }
    
    function setValueB(uint256 v) public virtual override {
        _valueB = v + 2; // Modified behavior
    }
}

contract DiamondMerge is ExtendedA {
    uint256 public finalValue;
    
    function setFinalValue(uint256 v) public {
        finalValue = v;
    }
    
    function setAll(uint256 v) public {
        setValueA(v);
        setValueB(v);
        setValueAB(v);
        setExtendedA(v);
        setFinalValue(v);
    }
    
    function sumAll() public view returns (uint256) {
        return _valueA + _valueB + _valueAB + _extendedA + finalValue;
    }
}

// ========== Multiple interface implementation ==========

interface IERC20 {
    function totalSupply() external view returns (uint256);
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
}

interface IERC20Metadata is IERC20 {
    function name() external view returns (string memory);
    function symbol() external view returns (string memory);
    function decimals() external view returns (uint8);
}

interface IOwnable {
    function owner() external view returns (address);
    function transferOwnership(address newOwner) external;
}

interface IPausable {
    function paused() external view returns (bool);
    function pause() external;
    function unpause() external;
}

abstract contract OwnableBase is IOwnable {
    address internal _owner;
    
    constructor() {
        _owner = msg.sender;
    }
    
    function owner() public view override returns (address) {
        return _owner;
    }
    
    function transferOwnership(address newOwner) public override {
        require(msg.sender == _owner, "Not owner");
        _owner = newOwner;
    }
    
    modifier onlyOwner() {
        require(msg.sender == _owner, "Not owner");
        _;
    }
}

abstract contract PausableBase is IPausable, OwnableBase {
    bool internal _paused;
    
    function paused() public view override returns (bool) {
        return _paused;
    }
    
    function pause() public override onlyOwner {
        _paused = true;
    }
    
    function unpause() public override onlyOwner {
        _paused = false;
    }
    
    modifier whenNotPaused() {
        require(!_paused, "Paused");
        _;
    }
}

contract MultiInterfaceToken is IERC20Metadata, PausableBase {
    string internal _name;
    string internal _symbol;
    uint256 internal _totalSupply;
    mapping(address => uint256) internal _balances;
    
    constructor(string memory name_, string memory symbol_) {
        _name = name_;
        _symbol = symbol_;
    }
    
    function name() public view override returns (string memory) { return _name; }
    function symbol() public view override returns (string memory) { return _symbol; }
    function decimals() public pure override returns (uint8) { return 18; }
    function totalSupply() public view override returns (uint256) { return _totalSupply; }
    function balanceOf(address account) public view override returns (uint256) { return _balances[account]; }
    
    function transfer(address to, uint256 amount) public override whenNotPaused returns (bool) {
        require(_balances[msg.sender] >= amount, "Insufficient balance");
        _balances[msg.sender] -= amount;
        _balances[to] += amount;
        return true;
    }
    
    function mint(address to, uint256 amount) public onlyOwner whenNotPaused {
        _balances[to] += amount;
        _totalSupply += amount;
    }
}

// ========== Virtual function override chain ==========

contract OverrideBase {
    uint256 public value;
    
    function compute(uint256 x) public virtual returns (uint256) {
        value = x;
        return x;
    }
}

contract Override1 is OverrideBase {
    function compute(uint256 x) public virtual override returns (uint256) {
        value = x + 1;
        return x + 1;
    }
}

contract Override2 is Override1 {
    function compute(uint256 x) public virtual override returns (uint256) {
        value = x + 2;
        return x + 2;
    }
}

contract Override3 is Override2 {
    function compute(uint256 x) public virtual override returns (uint256) {
        value = x + 3;
        return x + 3;
    }
}

contract Override4 is Override3 {
    function compute(uint256 x) public virtual override returns (uint256) {
        value = x + 4;
        return x + 4;
    }
}

contract OverrideFinal is Override4 {
    function compute(uint256 x) public override returns (uint256) {
        value = x + 5;
        return x + 5;
    }
    
    function callSuper(uint256 x) public returns (uint256) {
        return super.compute(x);
    }
}

// ========== Constructor chain ==========

contract ConstructorBase {
    uint256 public baseValue;
    
    constructor(uint256 v) {
        baseValue = v;
    }
}

contract ConstructorMiddle is ConstructorBase {
    uint256 public middleValue;
    
    constructor(uint256 b, uint256 m) ConstructorBase(b) {
        middleValue = m;
    }
}

contract ConstructorFinal is ConstructorMiddle {
    uint256 public finalValue;
    
    constructor(uint256 b, uint256 m, uint256 f) ConstructorMiddle(b, m) {
        finalValue = f;
    }
    
    function sumValues() public view returns (uint256) {
        return baseValue + middleValue + finalValue;
    }
}
