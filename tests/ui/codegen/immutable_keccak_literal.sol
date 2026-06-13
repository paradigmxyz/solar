//@compile-flags: -Zcodegen --emit=mir

contract ImmutableKeccakLiteral {
    bytes32 immutable value = keccak256("solar");

    function get() external view returns (bytes32) {
        return value;
    }
}
