//@ run-call: read() => 7, "ok"
//@ run-call: readArguments() => 7, "ok"

contract ExternalFunctionPointerAggregateTarget {
    function pair() external pure returns (uint256, string memory) {
        return (7, "ok");
    }

    function combine(uint256[] memory values, string memory text)
        external
        pure
        returns (uint256, string memory)
    {
        return (values[0] + values[1], text);
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

    function readArguments() external view returns (uint256, string memory) {
        function(uint256[] memory, string memory) external view returns (uint256, string memory)
            pointer = target.combine;
        uint256[] memory values = new uint256[](2);
        values[0] = 4;
        values[1] = 3;
        return pointer(values, "ok");
    }
}
