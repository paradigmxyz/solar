//@ compile-flags: --emit=bin --pretty-json
//@ filecheck:
//~? WARN: code generation is experimental

// CHECK-LABEL: "ROOT/tests/ui/cli/codegen.sol:C":
// CHECK: "bin":
// CHECK: "version":

contract C {
    function f() external {}
}
