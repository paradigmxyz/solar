//@ compile-flags: -Zcodegen -Zdump=mir-cfg
//@ filecheck: --check-prefix=DOT

contract DumpCfg {
    function f(uint x) public returns (uint) {
        // DOT: // === ROOT/tests/ui/codegen/mir/dump_cfg.sol:DumpCfg ===
        // DOT: digraph "f" {
        // DOT: node [shape=box
        // DOT: bb0 [label="bb0 (entry):\l
        // DOT: bb0 -> bb
        if (x == 0) {
            return 1;
        }
        return x;
    }
}
