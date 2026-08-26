// ported-from: test/libsolidity/syntaxTests/parsing/assembly_invalid_type.sol

contract C {
    function f() public pure {
        assembly "failasm" {} //~ ERROR: `evmasm` is the only supported assembly dialect
    }
}
