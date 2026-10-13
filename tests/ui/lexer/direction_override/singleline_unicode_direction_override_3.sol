// ported-from: test/libsolidity/syntaxTests/comments/singleline_unicode_direction_override_3.sol

contract C {
    function f() public pure
    {
        // RLO
        // overflow ‮
        //~^ ERROR: mismatching directional override markers in comment
    }
}
