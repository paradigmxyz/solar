library Lib {
    function externalTarget(uint256 value) external {
        value;
    }
}

error CustomError(uint256 value);

contract C {
    event CustomEvent(uint256 value);

    function internalTarget(uint256 value) internal pure {
        value;
    }

    function test(uint256 value, bytes memory data) public {
        abi.encodeCall(internalTarget, (value)); //~ ERROR: first argument to `abi.encodeCall` must be an external function
        abi.encodeCall(Lib.externalTarget, (value)); //~ ERROR: first argument to `abi.encodeCall` cannot be a library function
        abi.encodeCall(new C, (value)); //~ ERROR: first argument to `abi.encodeCall` cannot be a creation function
        abi.encodeCall(address(this).call, (data)); //~ ERROR: first argument to `abi.encodeCall` cannot be a special function
        abi.encodeCall(CustomEvent, (value)); //~ ERROR: first argument to `abi.encodeCall` cannot be an event
        abi.encodeCall(CustomError, (value)); //~ ERROR: first argument to `abi.encodeCall` cannot be an error
        abi.encodeCall(value, (value)); //~ ERROR: first argument to `abi.encodeCall` must be a function
    }
}
