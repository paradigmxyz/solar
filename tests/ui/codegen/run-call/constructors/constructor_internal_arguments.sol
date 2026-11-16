//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorInternalArguments::getFlag => true
//@ run-call: ConstructorInternalArguments::getName => 0x616263
// ported-from: test/libsolidity/semanticTests/constructor/constructor_arguments_internal.sol

contract ConstructorInternalArgumentsHelper {
    bytes3 internal name;
    bool internal flag;

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

contract ConstructorInternalArguments {
    ConstructorInternalArgumentsHelper internal helper;

    constructor() {
        helper = new ConstructorInternalArgumentsHelper("abc", true);
    }

    function getFlag() external view returns (bool) {
        return helper.getFlag();
    }

    function getName() external view returns (bytes3) {
        return helper.getName();
    }
}
