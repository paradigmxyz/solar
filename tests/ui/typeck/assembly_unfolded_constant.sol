// Inline assembly reads only constants with a value at compile time. Every use of another constant
// calls a helper that runs its initializer, which can allocate over memory the assembly uses.
contract D {}

contract C {
    uint256 constant HASH = uint256(keccak256(type(D).creationCode));
    uint256 constant FOLDED = uint256(keccak256("abc"));
    uint8 constant MAX8 = 255;
    uint8 constant WRAPPED = MAX8 + 1;
    // The branch that runs allocates, though the other one reverts.
    bytes32 constant BRANCH = true ? bytes32(HASH) : bytes32(uint256(WRAPPED));

    function f() external pure returns (uint256 r, uint256 s) {
        assembly {
            r := HASH //~ ERROR: inline assembly can only read constants with a compile-time value
            s := FOLDED
        }
    }

    // A constant whose initializer reverts has no value either, also inside a Yul function.
    function g() external pure returns (uint256 r) {
        assembly {
            function h() -> v {
                v := HASH //~ ERROR: inline assembly can only read constants with a compile-time value
            }
            r := add(WRAPPED, h()) //~ ERROR: inline assembly can only read constants with a compile-time value
            r := BRANCH //~ ERROR: inline assembly can only read constants with a compile-time value
        }
    }
}
