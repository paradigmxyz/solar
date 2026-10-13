// ported-from: test/libsolidity/syntaxTests/comments/multiline_unicode_direction_override_2.sol

contract C {
    function f() public pure
    {
        // PDF PDF
        /*underflow ‬‬*/
        //~^ ERROR: unicode direction override underflow in comment
    }
}
