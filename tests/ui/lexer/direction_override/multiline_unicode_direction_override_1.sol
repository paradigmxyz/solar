// ported-from: test/libsolidity/syntaxTests/comments/multiline_unicode_direction_override_1.sol

contract C {
    function f() public pure
    {
        // PDF
        /*underflow ‬*/
        //~^ ERROR: unicode direction override underflow in comment
    }
}
