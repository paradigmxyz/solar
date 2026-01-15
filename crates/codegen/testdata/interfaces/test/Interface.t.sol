// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Interface.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract InterfaceTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    Counter public counter;
    Caller public caller;

    function setUp() public {
        counter = Counter(_deployContract("COUNTER"));
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
            if (keccak256(bytes(name)) == keccak256("COUNTER")) {
                deployed = address(new Counter());
            } else if (keccak256(bytes(name)) == keccak256("CALLER")) {
                deployed = address(new Caller());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_CounterDirectIncrement() public {
        require(counter.count() == 0, "initial count should be 0");
        counter.increment();
        require(counter.count() == 1, "count should be 1 after increment");
    }

    function test_CounterMultipleIncrements() public {
        counter.increment();
        counter.increment();
        counter.increment();
        require(counter.count() == 3, "count should be 3 after 3 increments");
    }

    function test_CallThroughInterface() public {
        require(caller.getCount(address(counter)) == 0, "initial count should be 0");
        caller.callIncrement(address(counter));
        require(caller.getCount(address(counter)) == 1, "count should be 1 after increment via interface");
    }

    function test_MultipleCalls() public {
        caller.callIncrement(address(counter));
        caller.callIncrement(address(counter));
        require(caller.getCount(address(counter)) == 2, "count should be 2 after 2 increments via interface");
    }
}
