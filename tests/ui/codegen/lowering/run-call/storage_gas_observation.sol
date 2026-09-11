//@ codegen-matrix: standard
//@ run-call: overwrittenStore => 2
//@ run-call: forwardedLoad => 7

contract StorageGasObservation {
    uint256 value;

    function overwrittenStore() external returns (uint256) {
        uint256 before = gasleft();
        value = 1;
        uint256 spent = before - gasleft();
        if (spent > 10000) {
            value = 2;
        } else {
            value = 3;
        }
        return value;
    }

    function forwardedLoad() external returns (uint256 result) {
        value = 7;
        uint256 before = gasleft();
        result = value;
        uint256 spent = before - gasleft();
        if (spent <= 50) {
            result = 99;
        }
    }
}
