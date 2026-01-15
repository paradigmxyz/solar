// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Counter.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract CounterTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Counter public counter;

    function setUp() public {
        counter = Counter(_deployContract("COUNTER"));
    }

    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("COUNTER")) {
                deployed = address(new Counter());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_InitialCountIsZero() public view {
        require(counter.count() == 0, "initial count should be 0");
    }

    function test_Increment() public {
        counter.increment();
        require(counter.count() == 1, "count should be 1");
    }

    function test_IncrementTwice() public {
        counter.increment();
        counter.increment();
        require(counter.count() == 2, "count should be 2");
    }

    function test_GetCount() public {
        counter.increment();
        require(counter.getCount() == 1, "getCount should be 1");
    }
}
