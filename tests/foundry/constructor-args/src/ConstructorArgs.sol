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
    uint8 public immutable reassigned;
    int16 public immutable signed;
    bytes3 public immutable fixedBytes = bytes3(uint24(0xABCDEF));
    address public immutable account;
    Tiny public immutable userDefined;
    bool public immutable flag;
    function() internal pure returns (uint256) immutable functionPointer = immutableTarget;
    uint8 public observedBeforeReassignment;

    constructor(uint8 tiny_, int16 signed_, address account_, Tiny userDefined_, bool flag_) {
        tiny = tiny_;
        reassigned = tiny_;
        uint8 previous = reassigned;
        reassigned = tiny_ + 1;
        observedBeforeReassignment = previous;
        signed = signed_;
        account = account_;
        userDefined = userDefined_;
        flag = flag_;
    }

    function callFunctionPointer() external view returns (uint256) {
        return functionPointer();
    }

    function immutableTarget() internal pure returns (uint256) {
        return 7;
    }
}
