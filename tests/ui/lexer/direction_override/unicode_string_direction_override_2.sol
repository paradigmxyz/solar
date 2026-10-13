// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_2.sol

contract C {
    function f() public pure
    {
        // PDF PDF
        bytes memory m = unicode"underflow ‬‬";
        //~^ ERROR: unicode direction override underflow in string literal
    }
}
