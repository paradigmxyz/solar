// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract EventsTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    Events public events;

    event SimpleEvent(uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function setUp() public {
        events = Events(_deployContract("EVENTS"));
    }

    /// @dev Deploys a contract using Solar bytecode from env var, or falls back to solc
    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("EVENTS")) {
                deployed = address(new Events());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_EmitSimple() public {
        events.emitSimple(42);
    }

    function test_EmitTransfer() public {
        events.emitTransfer(address(0x1), address(0x2), 100);
    }
}
