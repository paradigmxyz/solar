// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_6.sol

contract C {
    function f() public pure
    {
        // PDF RLO
        bytes memory m = unicode" underflow ‬‮";
        //~^ ERROR: unicode direction override underflow in string literal
    }
}
