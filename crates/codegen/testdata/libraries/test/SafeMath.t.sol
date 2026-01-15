// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/SafeMath.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract SafeMathTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    TestLibrary public lib;

    function setUp() public {
        lib = TestLibrary(_deployContract("TESTLIBRARY"));
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
            if (keccak256(bytes(name)) == keccak256("TESTLIBRARY")) {
                deployed = address(new TestLibrary());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_safeAdd() public view {
        uint256 result = lib.safeAddDirect(1, 2);
        require(result == 3, "1 + 2 should equal 3");
    }

    function test_safeAddZero() public view {
        uint256 result = lib.safeAddDirect(0, 0);
        require(result == 0, "0 + 0 should equal 0");
    }

    function test_safeAddLarge() public view {
        uint256 result = lib.safeAddDirect(100, 200);
        require(result == 300, "100 + 200 should equal 300");
    }

    function test_safeSub() public view {
        uint256 result = lib.safeSubDirect(5, 3);
        require(result == 2, "5 - 3 should equal 2");
    }

    function test_safeSubLarge() public view {
        uint256 result = lib.safeSubDirect(100, 50);
        require(result == 50, "100 - 50 should equal 50");
    }

    function test_safeMul() public view {
        uint256 result = lib.safeMulDirect(3, 4);
        require(result == 12, "3 * 4 should equal 12");
    }

    function test_safeMulZero() public view {
        uint256 result = lib.safeMulDirect(0, 100);
        require(result == 0, "0 * 100 should equal 0");
    }

    function test_chainedOps() public view {
        // (2 + 3) * 4 = 20
        uint256 result = lib.chainedOps(2, 3, 4);
        require(result == 20, "(2 + 3) * 4 should equal 20");
    }

    function test_chainedOpsLarge() public view {
        // (10 + 5) * 2 = 30
        uint256 result = lib.chainedOps(10, 5, 2);
        require(result == 30, "(10 + 5) * 2 should equal 30");
    }
}
