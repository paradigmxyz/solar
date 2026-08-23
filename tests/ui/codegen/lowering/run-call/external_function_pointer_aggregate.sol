//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: read() => 7, "ok"
//@ run-call: readArguments() => 7, "ok"
//@ run-call: pointerArgumentRoundtrip() => 8
//@ run-call: pointerReturnRoundtrip() => 9
//@ run-call: pointerStructRoundtrip() => 9
//@ run-call: pointerArrayRoundtrip() => 9
//@ run-call: calldataPointerRoundtrip() => true

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
    struct PointerHolder {
        function(uint256) external returns (uint256) pointer;
    }

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

    function increment(uint256 value) external pure returns (uint256) {
        return value + 1;
    }

    function invoke(
        function(uint256) external returns (uint256) pointer,
        uint256 value
    )
        external
        returns (uint256)
    {
        return pointer(value);
    }

    function pointerArgumentRoundtrip() external view returns (uint256) {
        (bool success, bytes memory result) = address(this).staticcall(
            abi.encodeWithSelector(this.invoke.selector, this.increment, 7)
        );
        require(success);
        return abi.decode(result, (uint256));
    }

    function pointer()
        external
        view
        returns (function(uint256) external returns (uint256))
    {
        return this.increment;
    }

    function pointerReturnRoundtrip() external returns (uint256) {
        function(uint256) external returns (uint256) pointer = this.pointer();
        return pointer(8);
    }

    function pointerStructRoundtrip() external returns (uint256) {
        PointerHolder memory holder;
        holder.pointer = this.increment;
        return holder.pointer(8);
    }

    function pointerArrayRoundtrip() external returns (uint256) {
        function(uint256) external returns (uint256)[] memory pointers =
            new function(uint256) external returns (uint256)[](2);
        pointers[0] = this.increment;
        pointers[1] = this.increment;
        return pointers[0](3) + pointers[1](4);
    }

    function calldataTarget(string calldata text) external pure returns (bool) {
        return keccak256(bytes(text)) == keccak256("testString");
    }

    function calldataPointerRoundtrip() external returns (bool) {
        function(string memory) external returns (bool) pointer = this.calldataTarget;
        return pointer("testString");
    }
}
