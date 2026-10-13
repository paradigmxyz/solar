// ported-from: test/libsolidity/syntaxTests/literals/unicode_string_direction_override_7.sol

contract C {
    function f() public pure
    {
        // LRO PDF RLO PDF
        bytes memory m = unicode"‭ ok ‬‮‬";

        // lre rle pdf pdf
        m = unicode"lre‪ rle‫ pdf‬ pdf‬";
        // lre lro pdf pdf
        m = unicode"lre‪ lro‭ pdf‬ pdf‬";
    }
}
