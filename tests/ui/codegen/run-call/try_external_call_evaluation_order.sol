//@ run-call: f() => 1234

contract TryExternalCallEvaluationOrderTarget {
    function ping(uint256) external payable returns (uint256) {
        return 1;
    }
}

contract TryExternalCallEvaluationOrder {
    uint256 marker;
    TryExternalCallEvaluationOrderTarget private target;

    constructor() {
        target = new TryExternalCallEvaluationOrderTarget();
    }

    function receiver() internal returns (TryExternalCallEvaluationOrderTarget) {
        marker = marker * 10 + 1;
        return target;
    }

    function gasOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return gasleft();
    }

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 3;
        return 0;
    }

    function arg() internal returns (uint256) {
        marker = marker * 10 + 4;
        return 7;
    }

    function f() external returns (uint256) {
        try receiver().ping{gas: gasOpt(), value: valueOpt()}(arg()) returns (uint256) {
            return marker;
        } catch {
            return marker;
        }
    }
}
