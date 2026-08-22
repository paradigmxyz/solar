//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: getName(); constructor=[0x616263, true] => 0x616263
//@ run-call: getFlag(); constructor=[0x616263, true] => true
// ported-from: test/libsolidity/semanticTests/constructor/constructor_arguments_external.sol

contract ConstructorExternalArguments {
    bytes3 name;
    bool flag;

    constructor(bytes3 name_, bool flag_) {
        name = name_;
        flag = flag_;
    }

    function getName() external view returns (bytes3) {
        return name;
    }

    function getFlag() external view returns (bool) {
        return flag;
    }
}
