//@ codegen-matrix: standard
//@ run-call: overwrittenStore => 2
//@ run-call: forwardedLoad => 7
//@ run-call: diamond true => true
//@ run-call: diamond false => true
//@ run-call: loop => true

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

contract StorageGasCfgObservation {
    uint256 value;

    function diamond(bool observe) external view returns (bool result) {
        assembly {
            let warm := sload(0)
            let before := 0
            if observe { before := gas() }
            let loaded := sload(0)
            let after := gas()
            result := and(eq(warm, loaded), or(iszero(observe), gt(sub(before, after), 80)))
        }
    }

    function loop() external view returns (bool result) {
        assembly {
            let warm := sload(0)
            let before := 0
            result := 1
            for { let i := 0 } lt(i, 3) { i := add(i, 1) } {
                let loaded := sload(0)
                let after := gas()
                if i { result := and(result, gt(sub(before, after), 80)) }
                result := and(result, eq(warm, loaded))
                before := gas()
            }
        }
    }
}
