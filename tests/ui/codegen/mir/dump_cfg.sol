//@ compile-flags: -Zcodegen -Zdump=mir-cfg
//@ filecheck: --check-prefix=DOT

contract DumpCfg {
    // DOT: digraph "f" {
    // DOT: node [shape=box
    // DOT: bb0 [label="bb0:\l
    // DOT: bb0 -> bb
    function f(uint x) public pure returns (uint) {
        if (x == 0) {
            return 1;
        }
        return x;
    }
}
