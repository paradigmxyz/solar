// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ConstructorArgs.sol";

contract ConstructorArgsTest {
    ConstructorArgs public c;
    
    uint256 constant TEST_VALUE = 12345;
    address constant TEST_OWNER = address(0xBEEF);

    function buildString() public returns (ConstructorStringArgs, address) {
        return (new ConstructorStringArgs("test", address(this)), address(this));
    }

    function test_InternalStringConstructor() public {
        (ConstructorStringArgs instance, address owner) = buildString();
        require(keccak256(bytes(instance.name())) == keccak256("test"));
        require(instance.owner() == owner);
    }

    function setUp() public {
        c = new ConstructorArgs(TEST_VALUE, TEST_OWNER);
    }

    function test_ValueSet() public view {
        assert(c.value() == TEST_VALUE);
    }

    function test_OwnerSet() public view {
        assert(c.owner() == TEST_OWNER);
    }

    function test_GetValue() public view {
        assert(c.getValue() == TEST_VALUE);
    }

    function test_GetOwner() public view {
        assert(c.getOwner() == TEST_OWNER);
    }
}

contract ConstructorStringArgs {
    string public name;
    address public owner;
    event NameChanged(string previous, string current);

    constructor(string memory name_, address owner_) {
        setName(name_);
        owner = owner_;
    }

    function setName(string memory name_) internal {
        string memory previous = name;
        name = name_;
        emit NameChanged(previous, name_);
    }
}
