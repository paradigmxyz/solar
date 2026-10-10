// ported-from: test/libsolidity/syntaxTests/controlFlow/modifiers/non_implemented_modifier.sol
abstract contract A {
    function f() public view mod {
        require(block.timestamp > 10);
    }
    modifier mod() virtual;
}
