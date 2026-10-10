// ported-from: test/libsolidity/syntaxTests/fallback/arguments.sol
// ported-from: test/libsolidity/syntaxTests/fallback/fallback_wrong_data_location.sol
// ported-from: test/libsolidity/syntaxTests/fallback/no_input_no_output.sol
// ported-from: test/libsolidity/syntaxTests/fallback/return_value_number.sol
// ported-from: test/libsolidity/syntaxTests/fallback/return_value_type.sol
// ported-from: test/libsolidity/syntaxTests/fallback/return_value_unsupported.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/075_fallback_function_with_arguments.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/077_fallback_function_with_return_parameters.sol

contract Arguments {
    fallback(uint256) external {} //~ ERROR: invalid fallback function signature
}

contract MemoryInputMemoryOutput {
    fallback(bytes memory) external returns (bytes memory) {} //~ ERROR: invalid fallback function signature
}

contract MemoryInputCalldataOutput {
    fallback(bytes memory) external returns (bytes calldata) {} //~ ERROR: invalid fallback function signature
}

contract CalldataInputCalldataOutput {
    fallback(bytes calldata) external returns (bytes calldata) {} //~ ERROR: invalid fallback function signature
}

contract NoInput {
    fallback() external returns (bytes memory _output) {} //~ ERROR: invalid fallback function signature
}

contract NoOutput {
    fallback(bytes calldata _input) external {} //~ ERROR: invalid fallback function signature
}

contract TwoOutputs {
    fallback() external returns (bytes memory, bytes memory) {} //~ ERROR: invalid fallback function signature
}

contract UintOutput {
    fallback() external returns (uint256) {} //~ ERROR: invalid fallback function signature
}

contract OnlyBytesOutput {
    fallback() external returns (bytes memory) {} //~ ERROR: invalid fallback function signature
}

contract NamedArgument {
    uint x;
    fallback(uint a) external { x = 2; } //~ ERROR: invalid fallback function signature
}

contract UintReturn {
    fallback() external returns (uint) { } //~ ERROR: invalid fallback function signature
}

// The parameter's own error is enough.
contract Unresolved {
    fallback(Missing) external {} //~ ERROR: unresolved symbol `Missing`
}

contract Empty {
    fallback() external {}
}

contract BytesToBytes {
    fallback(bytes calldata input) external returns (bytes memory output) {
        output = input;
    }
}
