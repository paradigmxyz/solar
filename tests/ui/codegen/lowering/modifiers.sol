//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

contract ModifierLowering {
    uint256 private value;

    modifier outer(uint256 argument) {
        require(argument != 0);
        value = 1;
        _;
        value = 5;
    }

    modifier inner() {
        value = 2;
        _;
        value = 4;
    }

    // CHECK-LABEL: fn @run{{[(]}}
    // CHECK: sstore 0, 1
    // CHECK-NEXT: sstore 0, 2
    // CHECK-NEXT: sstore 0, 3
    // CHECK: mstore 128,
    // CHECK-NEXT: jump [[INNER_SUFFIX:bb[0-9]+]]
    // CHECK: [[OUTER_SUFFIX:bb[0-9]+]]:
    // CHECK-NEXT: sstore 0, 5
    // CHECK: [[INNER_SUFFIX]]:
    // CHECK-NEXT: sstore 0, 4
    // CHECK-NEXT: jump [[OUTER_SUFFIX]]
    function run(uint256 argument) external outer(argument) inner returns (uint256) {
        value = 3;
        return value;
    }
}
