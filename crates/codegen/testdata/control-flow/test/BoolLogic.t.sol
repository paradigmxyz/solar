// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/BoolLogic.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract BoolLogicTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    BoolLogic target;

    function setUp() public {
        target = BoolLogic(_deployContract("BOOLLOGIC"));
    }

    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("BOOLLOGIC")) {
                deployed = address(new BoolLogic());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    // ========== Pure function tests (no storage) ==========

    function test_pureAnd_ff() public view {
        require(target.pureAnd(false, false) == false, "ff should be false");
    }

    function test_pureAnd_ft() public view {
        require(target.pureAnd(false, true) == false, "ft should be false");
    }

    function test_pureAnd_tf() public view {
        require(target.pureAnd(true, false) == false, "tf should be false");
    }

    function test_pureAnd_tt() public view {
        require(target.pureAnd(true, true) == true, "tt should be true");
    }

    function test_pureOr_ff() public view {
        require(target.pureOr(false, false) == false, "ff should be false");
    }

    function test_pureOr_ft() public view {
        require(target.pureOr(false, true) == true, "ft should be true");
    }

    function test_pureOr_tf() public view {
        require(target.pureOr(true, false) == true, "tf should be true");
    }

    function test_pureOr_tt() public view {
        require(target.pureOr(true, true) == true, "tt should be true");
    }

    // ========== Storage tests ==========

    function test_storageAnd_ff() public {
        target.setFlags(false, false);
        require(target.testAnd() == false, "ff should be false");
    }

    function test_storageAnd_ft() public {
        target.setFlags(false, true);
        require(target.testAnd() == false, "ft should be false");
    }

    function test_storageAnd_tf() public {
        target.setFlags(true, false);
        require(target.testAnd() == false, "tf should be false");
    }

    function test_storageAnd_tt() public {
        target.setFlags(true, true);
        require(target.testAnd() == true, "tt should be true");
    }

    function test_storageOr_ff() public {
        target.setFlags(false, false);
        require(target.testOr() == false, "ff should be false");
    }

    function test_storageOr_ft() public {
        target.setFlags(false, true);
        require(target.testOr() == true, "ft should be true");
    }

    function test_storageOr_tf() public {
        target.setFlags(true, false);
        require(target.testOr() == true, "tf should be true");
    }

    function test_storageOr_tt() public {
        target.setFlags(true, true);
        require(target.testOr() == true, "tt should be true");
    }
}
