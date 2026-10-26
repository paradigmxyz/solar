// ported-from: test/libsolidity/syntaxTests/controlFlow/modifiers/implemented_without_placeholder.sol
abstract contract A {
    function f() public view mod {
        require(block.timestamp > 10);
    }
    modifier mod() virtual { } //~ ERROR: modifier must have a `_;` placeholder statement
}
