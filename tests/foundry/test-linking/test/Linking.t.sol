// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Child, Parent, NoArguments} from "../src/Child.sol";

interface Vm {
    function deal(address, uint256) external;
    function prank(address) external;
    function expectRevert(bytes calldata) external;
    function getCode(string calldata) external view returns (bytes memory);
}

contract LinkingTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testNamedStructAndValue() public {
        vm.deal(address(this), 9);
        Child child = (new Child){value: 9}({config: Child.Config(42, "native")});
        require(child.number() == 42);
        require(keccak256(bytes(child.name())) == keccak256("native"));
        require(child.creator() == address(this));
        require(child.paid() == 9);
    }

    function testNestedDeployments() public {
        Parent parent = new Parent(new Child(Child.Config(8, "nested")));
        require(parent.child().number() == 8);
    }

    function testCreate2Address() public {
        vm.deal(address(this), 1);
        bytes32 salt = keccak256("salt");
        Child.Config memory config = Child.Config(13, "salted");
        bytes memory init = bytes.concat(vm.getCode("Child.sol:Child"), abi.encode(config));
        address expected =
            address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(init))))));
        Child child = new Child{salt: salt, value: 1}(config);
        require(address(child) == expected);
        require(child.number() == 13);
        require(child.paid() == 1);
        Child other = new Child{salt: keccak256("other")}(config);
        require(other.number() == 13);
        require(other.paid() == 0);
    }

    function testPrankedConstructorCaller() public {
        vm.prank(address(0x1234));
        Child child = new Child(Child.Config(17, "pranked"));
        require(child.creator() == address(0x1234));
    }

    function testConstructorRevert() public {
        vm.expectRevert(abi.encodeWithSelector(Child.InvalidNumber.selector, uint256(0)));
        new Child(Child.Config(0, "revert"));
    }

    function testTryConstructorRemainsNative() public {
        try new Child(Child.Config(0, "caught")) returns (Child) {
            revert("unexpected success");
        } catch (bytes memory reason) {
            require(keccak256(reason) == keccak256(abi.encodeWithSelector(Child.InvalidNumber.selector, uint256(0))));
        }
    }

    function testCreationCode() public view {
        require(keccak256(type(Child).creationCode) == keccak256(vm.getCode("Child.sol:Child")));
    }

    function testNoArgumentOverloads() public {
        vm.deal(address(this), 2);
        require((new NoArguments()).paid() == 0);
        require((new NoArguments{value: 1}()).paid() == 1);
        require((new NoArguments{salt: keccak256("salt")}()).paid() == 0);
        require((new NoArguments{salt: keccak256("other"), value: 1}()).paid() == 1);
    }
}
