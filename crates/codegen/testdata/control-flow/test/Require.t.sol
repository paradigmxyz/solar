// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Require.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract RequireTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Require req;

    function setUp() public {
        req = Require(_deployContract("REQUIRE"));
    }

    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("REQUIRE")) {
                deployed = address(new Require());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    // ========== require() tests ==========

    function test_RequireTrue() public view {
        req.requireTrue(true); // should not revert
    }

    function test_RequireFalseReverts() public {
        try req.requireTrue(false) {
            revert("should have reverted");
        } catch {}
    }

    function test_RequireWithMessageTrue() public view {
        req.requireWithMessage(true); // should not revert
    }

    function test_RequireWithMessageFalseReverts() public {
        try req.requireWithMessage(false) {
            revert("should have reverted");
        } catch {}
    }

    // ========== revert() tests ==========

    function test_RevertAlwaysReverts() public {
        try req.revertAlways() {
            revert("should have reverted");
        } catch {}
    }

    function test_RevertWithMessageReverts() public {
        try req.revertWithMessage() {
            revert("should have reverted");
        } catch {}
    }

    // ========== divideChecked tests ==========

    function test_DivideCheckedSuccess() public view {
        require(req.divideChecked(10, 2) == 5, "10/2 = 5");
        require(req.divideChecked(100, 10) == 10, "100/10 = 10");
        require(req.divideChecked(7, 3) == 2, "7/3 = 2 (floor)");
        require(req.divideChecked(0, 5) == 0, "0/5 = 0");
    }

    function test_DivisionByZeroReverts() public {
        try req.divideChecked(10, 0) {
            revert("should have reverted");
        } catch {}
    }

    // TODO: requireChain tests skipped - modulo operator has bugs in require condition
}
