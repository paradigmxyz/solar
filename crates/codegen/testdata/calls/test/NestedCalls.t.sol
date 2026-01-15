// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/NestedCalls.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract NestedCallsTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    NestedCalls public nc;

    function setUp() public {
        nc = NestedCalls(_deployContract("NESTED_CALLS"));
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
            if (keccak256(bytes(name)) == keccak256("NESTED_CALLS")) {
                deployed = address(new NestedCalls());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function test_Add() public view {
        require(nc.add(5, 3) == 8, "5+3=8");
    }

    function test_Mul() public view {
        require(nc.mul(7, 6) == 42, "7*6=42");
    }

    function test_Nested2() public view {
        require(nc.nested2(3, 4, 5) == 17, "3*4+5=17");
    }

    function test_Nested3() public view {
        require(nc.nested3(1, 2, 3, 4) == 10, "(1+2)+(3+4)=10");
    }

    function test_DeepNested() public view {
        require(nc.deepNested(10) == 16, "((10+1)+2)+3=16");
    }

    // ========== External nested call tests ==========

    function test_Inner() public view {
        require(nc.inner(5) == 10, "inner(5) = 10");
    }

    function test_Outer() public view {
        // outer(x) = inner(inner(x)) = x * 2 * 2 = x * 4
        require(nc.outer(3) == 12, "outer(3) = 12");
        require(nc.outer(10) == 40, "outer(10) = 40");
    }

    function test_MixedBitwise() public view {
        // mixedBitwise(0xAB, 0xCD) = bitwiseOr(bitwiseAnd(0xAB, 0xF0), bitwiseAnd(0xCD, 0x0F))
        //                         = bitwiseOr(0xA0, 0x0D) = 0xAD
        require(nc.mixedBitwise(0xAB, 0xCD) == 0xAD, "mixedBitwise(AB, CD) = AD");
    }

    function test_NestedShifts() public view {
        // nestedShifts ignores arg, returns shiftRight(shiftLeft(1, 8), 4) = 256 >> 4 = 16
        require(nc.nestedShifts(0) == 16, "nestedShifts = 16");
    }
}
