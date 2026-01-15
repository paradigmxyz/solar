// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ExternalCall.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract ExternalCallTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    Callee public callee;
    Caller public caller;

    function setUp() public {
        callee = Callee(_deployContract("CALLEE"));
        caller = Caller(_deployContract("CALLER"));
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
            if (keccak256(bytes(name)) == keccak256("CALLEE")) {
                deployed = address(new Callee());
            } else if (keccak256(bytes(name)) == keccak256("CALLER")) {
                deployed = address(new Caller());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_DirectAdd() public view {
        uint256 result = callee.add(5, 3);
        require(result == 8, "direct add should return 8");
    }

    function test_DirectMultiply() public view {
        uint256 result = callee.multiply(7, 6);
        require(result == 42, "direct multiply should return 42");
    }

    function test_ExternalAdd() public view {
        uint256 result = caller.callAdd(address(callee), 5, 3);
        require(result == 8, "external add should return 8");
    }

    function test_ExternalMultiply() public view {
        uint256 result = caller.callMultiply(address(callee), 7, 6);
        require(result == 42, "external multiply should return 42");
    }

    function test_ChainedCalls() public view {
        uint256 result = caller.chainedCalls(address(callee), 5);
        require(result == 30, "chained calls should return 30");
    }
}
