// SPDX-License-Identifier: MIT
pragma solidity >=0.8.10;

import {Test} from "forge-std/Test.sol";

// Simplified Initializable matching OpenZeppelin's pattern
abstract contract Initializable {
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
            require(
                version == 1 && _isNotContract(),
                "Initializable: contract is already initialized"
            );
            return false;
        } else {
            require(_initialized < version, "Initializable: contract is already initialized");
            _initialized = version;
            return true;
        }
    }

    function _isNotContract() private view returns (bool) {
        return address(this).code.length == 0;
    }
}

contract MinimalInitializable is Initializable {
    address public token0;
    address public token1;

    function initialize(address _token0, address _token1) external initializer {
        token0 = _token0;
        token1 = _token1;
    }
}

contract MinimalInitializableTest is Test {
    MinimalInitializable pair;

    function setUp() public {
        pair = new MinimalInitializable();
    }

    function testInitialize() public {
        pair.initialize(address(1), address(2));
        assertEq(pair.token0(), address(1));
        assertEq(pair.token1(), address(2));
    }

    function testCannotReinitialize() public {
        pair.initialize(address(1), address(2));
        vm.expectRevert("Initializable: contract is already initialized");
        pair.initialize(address(3), address(4));
    }
}
