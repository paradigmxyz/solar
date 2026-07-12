interface Target {
    function takesMemoryFunction(function(string memory) external callback) external;
    function takesCalldataFunction(function(string calldata) external callback) external;
    function takesCalldataString(string calldata value) external;
    function takesMemoryString(string memory value) external;
    function takesPayableAddress(address payable value) external;
}

contract C {
    string stored;

    function calldataString(string calldata value) external {
        value;
    }

    function memoryString(string memory value) external {
        value;
    }

    function test(string memory value) external view {
        abi.encodeCall(Target.takesMemoryFunction, (this.calldataString));
        abi.encodeCall(Target.takesMemoryFunction, (this.memoryString));
        abi.encodeCall(Target.takesCalldataString, (value));
        abi.encodeCall(Target.takesCalldataString, (stored));
        abi.encodeCall(Target.takesMemoryString, (stored));

        abi.encodeCall(Target.takesPayableAddress, (address(0))); //~ ERROR: mismatched types
        abi.encodeCall(Target.takesCalldataFunction, (this.calldataString)); //~ ERROR: mismatched types
        abi.encodeCall(Target.takesCalldataFunction, (this.memoryString)); //~ ERROR: mismatched types
    }
}
