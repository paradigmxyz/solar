// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/DelegatecallGuard.sol";

interface Vm {
    function getCode(string calldata artifact) external returns (bytes memory);
}

/// A library's non-view external functions run only through `DELEGATECALL`: a direct call
/// reverts, while view and pure functions accept direct calls, with or without value.
contract DelegatecallGuardTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    GuardUser user;
    address lib;

    function setUp() public {
        user = new GuardUser();
        bytes memory code = vm.getCode("DelegatecallGuard.sol:GuardLib");
        address deployed;
        assembly {
            deployed := create(0, add(code, 0x20), mload(code))
        }
        require(deployed != address(0), "library deployment failed");
        lib = deployed;
    }

    function test_delegatecallAcceptsValue() public {
        require(user.bump{value: 1 ether}(7) == 1, "bump");
        require(user.bump(8) == 2, "second bump");
        require(user.peek() == 2, "peek");
    }

    function test_directNonViewCallReverts() public {
        (bool ok, bytes memory data) = lib.call(abi.encodeWithSelector(GuardLib.bump.selector, 0, 1));
        require(!ok, "direct non-view call succeeded");
        require(data.length == 0, "unexpected revert data");
    }

    function test_directViewCallSucceeds() public {
        (bool ok, bytes memory data) = lib.call(abi.encodeWithSelector(GuardLib.peek.selector, 0));
        require(ok, "direct view call failed");
        require(abi.decode(data, (uint256)) == 0, "peek");

        (ok, data) = lib.call{value: 1}(abi.encodeWithSelector(GuardLib.twice.selector, 21));
        require(ok, "direct pure call with value failed");
        require(abi.decode(data, (uint256)) == 42, "twice");
    }
}
