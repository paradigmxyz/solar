// Inline assembly reads only constants with a compile-time value. Any other constant runs its
// initializer at every use, which can allocate over memory the assembly uses.
contract D {}

contract C {
    uint256 constant HASH = uint256(keccak256(type(D).creationCode));
    uint256 constant FOLDED = uint256(keccak256("abc"));

    function f() external pure returns (uint256 r, uint256 s) {
        assembly {
            r := HASH //~ ERROR: only direct number constants are supported in inline assembly
            s := FOLDED
        }
    }
}
