//@compile-flags: -Zcodegen -Zdump=mir

contract ImmutableKeccakLiteral {
    bytes32 immutable value = keccak256("solar");

    function get() external view returns (bytes32) {
        return value;
    }
}
