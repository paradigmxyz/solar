//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: ConstructorModifierStorageVirtual::result() => 1, 2

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
