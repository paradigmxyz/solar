// ported-from: test/libsolidity/syntaxTests/comments/multiline_unicode_direction_override_6.sol

contract C {
    function f() public pure
    {
        // PDF RLO
        /*overflow ‬‮*/
        //~^ ERROR: unicode direction override underflow in comment
    }
}
