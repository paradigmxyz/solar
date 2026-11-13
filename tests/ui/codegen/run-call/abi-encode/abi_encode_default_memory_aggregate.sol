//@ codegen-matrix: standard
//@ run-call: hashStaticTuple => 0x3cbeb3335496f50b51298eb53dfb7ca46fe5c1094fc08f3b4728f83eebbf5d08
//@ run-call: hashDynamicTuple => 0xc54789984a16abd069aaedb7fe85d38530851497df99398d3bc73e44f1f3968f
//@ run-call: hashStaticFixedArray => 0x3cbeb3335496f50b51298eb53dfb7ca46fe5c1094fc08f3b4728f83eebbf5d08
//@ run-call: hashDynamicFixedArray => 0x67a0727280fa47fae4193220588a798f7c160a5caf8cb69cf5eedcc4b6756357
//@ run-call: hashNestedTuple => 0x277678f2d57c978edfc4258934aeb41d3798e31d92363e5a6e647771c96bbadc
//@ run-call: returnDefaultSmall => [(0)]

contract AbiEncodeDefaultMemoryAggregates {
    struct StaticTuple {
        uint256 first;
        uint256 second;
        uint256 third;
    }

    struct DynamicTuple {
        uint256 value;
        bytes data;
        uint256[] values;
    }

    struct NestedTuple {
        StaticTuple inner;
        bytes data;
    }

    struct Small {
        uint8 value;
    }

    function hashStaticTuple() external pure returns (bytes32) {
        StaticTuple[] memory values = new StaticTuple[](1);
        return keccak256(abi.encode(values));
    }

    function hashDynamicTuple() external pure returns (bytes32) {
        DynamicTuple[] memory values = new DynamicTuple[](1);
        return keccak256(abi.encode(values));
    }

    function hashStaticFixedArray() external pure returns (bytes32) {
        uint256[3][] memory values = new uint256[3][](1);
        return keccak256(abi.encode(values));
    }

    function hashDynamicFixedArray() external pure returns (bytes32) {
        bytes[3][] memory values = new bytes[3][](1);
        return keccak256(abi.encode(values));
    }

    function hashNestedTuple() external pure returns (bytes32) {
        NestedTuple[] memory values = new NestedTuple[](1);
        return keccak256(abi.encode(values));
    }

    function returnDefaultSmall() external pure returns (Small[] memory values) {
        assembly ("memory-safe") {
            mstore(0, 0x123)
        }
        values = new Small[](1);
    }
}
