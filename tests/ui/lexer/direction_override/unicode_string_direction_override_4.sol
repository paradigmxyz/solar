// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_4.sol

contract C {
    function f() public pure
    {
        // RLO RLO
        bytes memory m = unicode"overflow ‮‮";
        //~^ ERROR: mismatching directional override markers in string literal
    }
}
