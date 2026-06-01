// SPDX-License-Identifier: MIT
pragma solidity >=0.8.10;

import {Test} from "forge-std/Test.sol";

// Minimal ERC20-like base
abstract contract SimpleERC20 {
    string public name;
    string public symbol;
    uint8 public decimals;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    constructor(string memory _name, string memory _symbol, uint8 _decimals) {
        name = _name;
        symbol = _symbol;
        decimals = _decimals;
    }
}

// Minimal ReentrancyGuard-like
abstract contract SimpleReentrancyGuard {
    uint256 private locked = 1;

    modifier nonReentrant() {
        require(locked == 1, "REENTRANCY");
        locked = 2;
        _;
        locked = 1;
    }
}

// Minimal Initializable
abstract contract SimpleInitializable {
    uint8 private _initialized;
    bool private _initializing;

    event Initialized(uint8 version);

    modifier initializer() {
        bool isTopLevelCall = _setInitializedVersion(1);
        if (isTopLevelCall) {
            _initializing = true;
        }
        _;
        if (isTopLevelCall) {
            _initializing = false;
            emit Initialized(1);
        }
    }

    function _setInitializedVersion(uint8 version) private returns (bool) {
        if (_initializing) {
            require(version == 1 && address(this).code.length == 0, "already initialized");
            return false;
        } else {
            require(_initialized < version, "already initialized");
            _initialized = version;
            return true;
        }
    }
}

// Multi-inheritance like UnifapV2Pair
contract MultiInheritPair is SimpleERC20, SimpleReentrancyGuard, SimpleInitializable {
    address public token0;
    address public token1;

    constructor() SimpleERC20("Test", "TST", 18) {}

    function initialize(address _token0, address _token1) external initializer {
        token0 = _token0;
        token1 = _token1;
    }

    function doSomething() public nonReentrant returns (uint256) {
        return 42;
    }
}

contract MultiInheritInitTest is Test {
    MultiInheritPair pair;

    function setUp() public {
        pair = new MultiInheritPair();
    }

    function testInitialize() public {
        pair.initialize(address(1), address(2));
        assertEq(pair.token0(), address(1));
        assertEq(pair.token1(), address(2));
    }

    function testNonReentrant() public {
        assertEq(pair.doSomething(), 42);
    }
}
