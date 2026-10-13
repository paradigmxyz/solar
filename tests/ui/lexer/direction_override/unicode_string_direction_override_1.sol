// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_1.sol

contract C {
    function f() public pure
    {
        // PDF
        bytes memory s = unicode"underflow ‬";
        //~^ ERROR: unicode direction override underflow in string literal
    }
}
