//@ compile-flags: --stop-after parsing
// Expression trees are limited to 2500 levels; see PARSE-007.
contract C {
    function f(uint a) public pure returns (uint) {
        return
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^ // 50
        a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a^a;
    }
}
