//@ compile-flags: -Zcodegen -Zdump=mir-cfg
//@ filecheck: --enable-var-scope

contract DumpCfg {
    // CHECK-LABEL: digraph "f" {
    // CHECK: node [shape=box
    // CHECK: [[ENTRY:bb[0-9]+]] [label="[[ENTRY]]:\l
    // CHECK: [[ENTRY]] -> bb
    function f(uint x) public pure returns (uint) {
        if (x == 0) {
            return 1;
        }
        return x;
    }

    // CHECK-LABEL: digraph "storageOps" {
    // CHECK-NOT: metadata
    // CHECK: sstore arg0, arg1\l
    // CHECK-NOT: metadata
    // CHECK: [[LOAD:v[0-9]+]] = sload arg0\l
    // CHECK-NOT: metadata
    function storageOps(uint slot, uint value) public returns (uint loaded) {
        assembly {
            sstore(slot, value)
            loaded := sload(slot)
        }
    }
}
