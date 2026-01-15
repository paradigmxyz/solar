// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/DynamicArray.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function envBytes(string calldata key) external view returns (bytes memory);
}

contract DynamicArrayTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    DynamicArray arr;

    function _deployContract(string memory name) internal returns (address deployed) {
        string memory envKey = string.concat("SOLAR_", name, "_BYTECODE");
        try vm.envBytes(envKey) returns (bytes memory creationCode) {
            assembly {
                deployed := create(0, add(creationCode, 0x20), mload(creationCode))
            }
            require(deployed != address(0), string.concat("Solar deploy failed: ", name));
        } catch {
            if (keccak256(bytes(name)) == keccak256("DYNAMICARRAY")) {
                deployed = address(new DynamicArray());
            } else {
                revert(string.concat("Unknown contract: ", name));
            }
        }
    }

    function setUp() public {
        arr = DynamicArray(_deployContract("DYNAMICARRAY"));
    }

    function test_initialLength() public view {
        require(arr.length() == 0, "initial length should be 0");
    }

    function test_push() public {
        arr.push(42);
        require(arr.length() == 1, "length should be 1");
        require(arr.get(0) == 42, "value at 0 should be 42");
    }

    function test_pushMultiple() public {
        arr.push(10);
        arr.push(20);
        arr.push(30);
        require(arr.length() == 3, "length should be 3");
        require(arr.get(0) == 10, "value at 0 should be 10");
        require(arr.get(1) == 20, "value at 1 should be 20");
        require(arr.get(2) == 30, "value at 2 should be 30");
    }

    function test_pop() public {
        arr.push(100);
        arr.push(200);
        require(arr.length() == 2, "length should be 2");

        arr.pop();
        require(arr.length() == 1, "length after pop should be 1");
        require(arr.get(0) == 100, "value at 0 should be 100");
    }

    function test_pushPop() public {
        arr.push(1);
        arr.push(2);
        arr.push(3);
        require(arr.length() == 3, "length should be 3");

        arr.pop();
        require(arr.length() == 2, "length after pop should be 2");

        arr.push(4);
        require(arr.length() == 3, "length after push should be 3");
        require(arr.get(2) == 4, "value at 2 should be 4");
    }

    function test_pushMultipleFunction() public {
        arr.pushMultiple(111, 222, 333);
        require(arr.length() == 3, "length should be 3");
        require(arr.get(0) == 111, "value at 0 should be 111");
        require(arr.get(1) == 222, "value at 1 should be 222");
        require(arr.get(2) == 333, "value at 2 should be 333");
    }
}
