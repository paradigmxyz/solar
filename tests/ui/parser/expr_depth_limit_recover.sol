//@ compile-flags: --stop-after parsing -Zrecover-incomplete-input
// Expression trees are limited to 2500 levels; see PARSE-007.
contract C {
    function f(uint a) public pure {
        a
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        (-) //~ ERROR: expected one of
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b //~ ERROR: expression is too deeply nested
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        .b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b.b // 50
        ;
    }
}
