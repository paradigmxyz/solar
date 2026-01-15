// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/MultiReturn.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract MultiReturnTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    MultiReturn public multiReturn;

    function setUp() public {
        multiReturn = MultiReturn(_deployContract("MULTIRETURN"));
    }

    /// @dev Deploys a contract using Solar bytecode from env var, or falls back to solc
    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deployment failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("MULTIRETURN")) {
                deployed = address(new MultiReturn());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_DirectGetTwo() public view {
        (uint256 a, uint256 b) = multiReturn.getTwo();
        require(a == 1, "getTwo first value should be 1");
        require(b == 2, "getTwo second value should be 2");
    }

    function test_DirectGetThree() public view {
        (uint256 a, uint256 b, uint256 c) = multiReturn.getThree();
        require(a == 10, "getThree first value should be 10");
        require(b == 20, "getThree second value should be 20");
        require(c == 30, "getThree third value should be 30");
    }

    function test_MultiReturn() public view {
        (uint256 a, uint256 b) = multiReturn.testTwo();
        require(a == 1, "testTwo first value should be 1");
        require(b == 2, "testTwo second value should be 2");
    }

    function test_MultiReturnThree() public view {
        (uint256 a, uint256 b, uint256 c) = multiReturn.testThree();
        require(a == 10, "testThree first value should be 10");
        require(b == 20, "testThree second value should be 20");
        require(c == 30, "testThree third value should be 30");
    }

    function test_MultiReturnConditional() public view {
        uint256 b = multiReturn.testPartialCapture();
        require(b == 2, "partial capture should return 2");
    }

    function test_SimpleReturn() public view {
        (uint256 a, uint256 b) = multiReturn.simpleReturn();
        require(a == 111, "simpleReturn first value should be 111");
        require(b == 222, "simpleReturn second value should be 222");
    }

    function test_TestSimpleReturn() public view {
        (uint256 a, uint256 b) = multiReturn.testSimpleReturn();
        require(a == 111, "testSimpleReturn first value should be 111");
        require(b == 222, "testSimpleReturn second value should be 222");
    }

    function test_MultiReturnViaCaller() public view {
        (uint256 a, uint256 b) = multiReturn.callVia(address(multiReturn));
        require(a == 1, "via caller first value should be 1");
        require(b == 2, "via caller second value should be 2");
    }
}
