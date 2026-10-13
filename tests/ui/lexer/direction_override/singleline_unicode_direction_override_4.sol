// ported-from: test/libsolidity/syntaxTests/comments/singleline_unicode_direction_override_4.sol

contract C {
    function f() public pure
    {
        // RLO RLO
        // overflow ‮‮
        //~^ ERROR: mismatching directional override markers in comment
    }
}
