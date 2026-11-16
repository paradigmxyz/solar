//@ codegen-matrix: standard
//@ run-call: repeated 1 => 1048576
//@ run-call: repeated 2 => 2097152
//@ run-call: run 1 => 1048576

contract HeapPrefixSharedExpression {
    function run(uint256 value) external pure returns (uint256) {
        return repeated(value);
    }

    function repeated(uint256 value) public pure returns (uint256) {
        // Shared operands exercise failure caching in heap-prefix analysis.
        assembly {
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
            value := add(value, value)
        }
        return value;
    }
}
