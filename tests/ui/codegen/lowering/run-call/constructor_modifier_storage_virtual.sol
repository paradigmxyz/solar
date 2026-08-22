//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: ConstructorModifierStorageVirtual::result() => 1, 2
//@[gas] run-call: ConstructorModifierStorageVirtual::result() => 1, 2
//@[size] run-call: ConstructorModifierStorageVirtual::result() => 1, 2

contract ConstructorModifierStorageVirtualBase {
    uint256[] internal values;

    constructor() initialize(values) {}

    modifier initialize(uint256[] storage target) {
        target.push(value());
        _;
    }

    function value() internal pure virtual returns (uint256) {
        return 1;
    }
}

contract ConstructorModifierStorageVirtual is ConstructorModifierStorageVirtualBase {
    function value() internal pure override returns (uint256) {
        return 2;
    }

    function result() external view returns (uint256, uint256) {
        return (values.length, values[0]);
    }
}
