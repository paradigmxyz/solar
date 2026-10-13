// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_5.sol

contract C {
    function f() public pure
    {
        // RLO PDF
        bytes memory m = unicode" ok ‮‬";

        // RLO RLO PDF PDF
        m = unicode" ok ‮‮‬‬";

        // RLO RLO RLO PDF PDF PDF
        m = unicode" ok ‮‮‮‬‬‬";
    }
}
