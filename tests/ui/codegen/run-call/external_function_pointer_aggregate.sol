//@ run-call: read() => 7, "ok"

contract ExternalFunctionPointerAggregateTarget {
    function pair() external pure returns (uint256, string memory) {
        return (7, "ok");
    }
}

contract ExternalFunctionPointerAggregate {
    ExternalFunctionPointerAggregateTarget private target;

    constructor() {
        target = new ExternalFunctionPointerAggregateTarget();
    }

    function read() external view returns (uint256, string memory) {
        function() external view returns (uint256, string memory) pointer = target.pair;
        return pointer();
    }
}
