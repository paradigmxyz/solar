//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorFunctionCallFixedBytes::getName => 0x616263
// ported-from: test/libsolidity/semanticTests/constructor/functions_called_by_constructor.sol

contract ConstructorFunctionCallFixedBytes {
    bytes3 internal name;

    constructor() {
        setName("abc");
    }

    function getName() external view returns (bytes3) {
        return name;
    }

    function setName(bytes3 name_) private {
        name = name_;
    }
}
