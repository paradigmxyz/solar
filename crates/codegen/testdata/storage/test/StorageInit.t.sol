// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StorageInit.sol";

interface Vm { function envBytes(string calldata) external view returns (bytes memory); }

contract StorageInitTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    StorageInit public s;

    function _deploy(string memory n) internal returns (address d) {
        try vm.envBytes(string.concat("SOLAR_", n, "_BYTECODE")) returns (bytes memory c) {
            assembly { d := create(0, add(c, 0x20), mload(c)) }
        } catch { d = address(new StorageInit()); }
    }

    function setUp() public {
        s = StorageInit(_deploy("STORAGEINIT"));
    }

    function test_ValueInitialized() public view {
        require(s.value() == 42, "value should be 42");
    }

    function test_AnotherValueInitialized() public view {
        require(s.anotherValue() == 100, "anotherValue should be 100");
    }

    function test_GetValue() public view {
        require(s.getValue() == 42, "getValue should return 42");
    }
}
