// ported-from: test/libsolidity/syntaxTests/comments/singleline_unicode_direction_override_6.sol

contract C {
    function f() public pure
    {
        // PDF RLO
        // underflow ‬‮
        //~^ ERROR: unicode direction override underflow in comment
    }
}
