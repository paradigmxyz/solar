//@ codegen-matrix: standard
//@ filecheck:
//@ run-call: arithmetic 4 => 6
//@ run-call: viewCall => 2
// A storage array copies where it is evaluated when no later operand can write storage.
contract Test {
    uint256[] sa;

    constructor() {
        sa.push(1);
    }

    function first(uint256[] memory m, uint256 z) internal pure returns (uint256) {
        return m[0] + z;
    }

    function length() internal view returns (uint256) {
        return sa.length;
    }

    // CHECK-LABEL: fn @arithmetic
    // CHECK: icall @load_storage_word_array
    // CHECK: checked_add
    function arithmetic(uint256 x) external view returns (uint256) {
        return first(sa, x + 1);
    }

    // CHECK-LABEL: fn @viewCall
    // CHECK: icall @load_storage_word_array
    // CHECK: icall @length
    function viewCall() external view returns (uint256) {
        return first(sa, length());
    }
}
