// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_3.sol

contract C {
    function f() public pure
    {
        // RLO
        bytes memory m = unicode"overflow ‮";
        //~^ ERROR: mismatching directional override markers in string literal
    }
}
