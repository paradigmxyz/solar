// Inline assembly reads only constants with a value at compile time. Every use of another constant
// calls a helper that runs its initializer, which can allocate over memory the assembly uses.
contract D {}

contract C {
    uint256 constant HASH = uint256(keccak256(type(D).creationCode));
    uint256 constant FOLDED = uint256(keccak256("abc"));
    uint8 constant MAX8 = 255;
    uint8 constant WRAPPED = MAX8 + 1;

    function f() external pure returns (uint256 r, uint256 s) {
        assembly {
            r := HASH //~ ERROR: only direct number constants are supported in inline assembly
            s := FOLDED
        }
    }

    // A constant whose initializer reverts has no value either, also inside a Yul function.
    function g() external pure returns (uint256 r) {
        assembly {
            function h() -> v {
                v := HASH //~ ERROR: only direct number constants are supported in inline assembly
            }
            r := add(WRAPPED, h()) //~ ERROR: only direct number constants are supported in inline assembly
        }
    }
}
