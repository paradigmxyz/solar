// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ConstructorArgs {
    uint256 public value;
    address public owner;
    
    constructor(uint256 _value, address _owner) {
        value = _value;
        owner = _owner;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
    
    function getOwner() external view returns (address) {
        return owner;
    }
}

type Tiny is uint16;

contract ImmutableArgs {
    uint8 public immutable tiny;
    int16 public immutable signed;
    bytes3 public immutable fixedBytes = bytes3(uint24(0xABCDEF));
    address public immutable account;
    Tiny public immutable userDefined;

    constructor(uint8 tiny_, int16 signed_, address account_, Tiny userDefined_) {
        tiny = tiny_;
        signed = signed_;
        account = account_;
        userDefined = userDefined_;
    }
}
