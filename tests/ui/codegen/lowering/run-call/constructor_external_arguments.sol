//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: getName(); constructor=[0x616263, true] => 0x616263
//@[none, gas, size] run-call: getFlag(); constructor=[0x616263, true] => true
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
