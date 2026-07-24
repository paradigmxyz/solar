//@ compile-flags: -Zcodegen -Zdump=mir-cfg
//@ filecheck: --enable-var-scope

contract DumpCfg {
    // CHECK-LABEL: digraph "f" {
    // CHECK: node [shape=box
    // CHECK: [[BRANCH:bb[0-9]+]] [label="[[BRANCH]]:\l{{.*}}[[COND:v[0-9]+]] = eq arg0, 0\l  jumpi [[COND]], [[THEN:bb[0-9]+]], [[ELSE:bb[0-9]+]]\l"];
    // CHECK: [[BRANCH]] -> [[THEN]] [label="[[COND]] == true", color="green"];
    // CHECK-NEXT: [[BRANCH]] -> [[ELSE]] [label="false", color="red"];
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
    // CHECK: {{v[0-9]+}} = sload arg0\l
    // CHECK-NOT: metadata
    function storageOps(uint slot, uint value) public returns (uint loaded) {
        assembly {
            sstore(slot, value)
            loaded := sload(slot)
        }
    }
}
