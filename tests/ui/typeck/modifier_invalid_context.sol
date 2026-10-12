// ported-from: test/libsolidity/syntaxTests/modifiers/use_in_invalid_context.sol

contract test {
    modifier mod() { _; }

    function f() public {
        mod  ; //~ ERROR: modifier can only be referenced in function headers
    }
}
