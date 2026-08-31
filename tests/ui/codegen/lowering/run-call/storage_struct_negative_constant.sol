//@ codegen-matrix: standard
//@ run-call: assign() => -1

contract StorageStructNegativeConstant {
    struct Value {
        int256 value;
    }

    Value internal value;

    function assign() external returns (int256) {
        value = Value(-1);
        return value.value;
    }
}
