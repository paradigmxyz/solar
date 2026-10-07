//@ codegen-matrix: standard
//@ run-call: repeated 1 => 268435456
//@ run-call: repeated 2 => 536870912
//@ run-call: run 1 => 268435456

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
