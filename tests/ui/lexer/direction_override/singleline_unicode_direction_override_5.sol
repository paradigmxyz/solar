// ported-from: test/libsolidity/syntaxTests/comments/singleline_unicode_direction_override_5.sol

contract C {
    function f() public pure
    {
        // RLO PDF
        // ok ‮‬

        // RLO RLO PDF PDF
        // ok ‮‮‬‬

        // RLO RLO RLO PDF PDF PDF
        // ok ‮‮‮‬‬‬
    }
}
