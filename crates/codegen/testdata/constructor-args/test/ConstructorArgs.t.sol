// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ConstructorArgs.sol";

interface Vm {
    function envBytes(string calldata) external view returns (bytes memory);
}

contract ConstructorArgsTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    ConstructorArgs public c;
    
    uint256 constant TEST_VALUE = 12345;
    address constant TEST_OWNER = address(0xBEEF);

    function _deploy(string memory n, uint256 _value, address _owner) internal returns (address d) {
        try vm.envBytes(string.concat("SOLAR_", n, "_BYTECODE")) returns (bytes memory code) {
            // Append ABI-encoded constructor args to the bytecode
            bytes memory initcode = abi.encodePacked(code, abi.encode(_value, _owner));
            assembly {
                d := create(0, add(initcode, 0x20), mload(initcode))
            }
        } catch {
            d = address(new ConstructorArgs(_value, _owner));
        }
    }

    function setUp() public {
        c = ConstructorArgs(_deploy("CONSTRUCTORARGS", TEST_VALUE, TEST_OWNER));
    }

    function test_ValueSet() public view {
        require(c.value() == TEST_VALUE, "value should be set by constructor");
    }

    function test_OwnerSet() public view {
        require(c.owner() == TEST_OWNER, "owner should be set by constructor");
    }

    function test_GetValue() public view {
        require(c.getValue() == TEST_VALUE, "getValue should return constructor value");
    }

    function test_GetOwner() public view {
        require(c.getOwner() == TEST_OWNER, "getOwner should return constructor owner");
    }
}
