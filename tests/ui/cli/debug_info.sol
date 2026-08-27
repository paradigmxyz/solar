//@ compile-flags: -g
//@ filecheck:
//~? WARN: code generation is experimental

// CHECK-LABEL: {{^}}=== Debug Data (ethdebug/format/info/resources) ===
// CHECK: "compiler":{"name":"solar"
// CHECK: "path":"ROOT/tests/ui/cli/debug_info.sol"
// CHECK-LABEL: {{^}}=== Debug Data (ethdebug/format/program, creation):
// CHECK: "environment":"create"
// CHECK-LABEL: {{^}}=== Debug Data (ethdebug/format/program, runtime):
// CHECK: "environment":"call"
// CHECK: "mnemonic":"ADD"
contract DebugInfo {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
