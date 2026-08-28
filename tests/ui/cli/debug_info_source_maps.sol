//@ compile-flags: -g source-maps
//@ filecheck:
//~? WARN: code generation is experimental

// CHECK: "srcmap":":::-:0
// CHECK: "srcmap-runtime":":::-:0
// CHECK: :::i
// CHECK: :::o
// CHECK: "sourceList":["ROOT/tests/ui/cli/debug_info_source_maps.sol"]
contract DebugInfoSourceMaps {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
