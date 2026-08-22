//@ run-call: hasCode() => true
//@ run-call: rejectsCalls() => true

contract Empty {}

contract EmptyContractRuntime {
    function hasCode() external returns (bool) {
        Empty empty = new Empty();
        return address(empty).code.length != 0;
    }

    function rejectsCalls() external returns (bool) {
        Empty empty = new Empty();
        (bool success,) = address(empty).call("");
        return !success;
    }
}
