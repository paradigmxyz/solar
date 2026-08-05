//@ run-call-fail: 0x46e5c92b0000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0xf3a326f80000000000000000000000000000000000000000000000000000000000000020
//@ run-call: 0x4cdbec5f0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000dead0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000001

contract AbiCalldataUnusedAggregateValidation {
    struct Values {
        uint256 first;
        bytes second;
        uint256 third;
    }

    struct Nested {
        uint256[][] first;
        uint256 second;
    }

    function unusedBytes(bytes calldata) external pure returns (uint256) {
        return 1;
    }

    function unusedStruct(Values calldata) external pure returns (uint256) {
        return 1;
    }

    function unusedNested(Nested calldata) external pure returns (uint256) {
        return 1;
    }
}
