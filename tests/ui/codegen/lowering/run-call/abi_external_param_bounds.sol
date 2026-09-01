//@ codegen-matrix: standard
//@ run-call: Bounds::bytesMemory(bytes) 0x616263 => 3
//@ run-call: Bounds::wordsMemory(uint256[]) [1, 2, 3] => 3
//@ run-call: Bounds::bytesCalldata(bytes) 0x61626364 => 4
//@ run-call: Bounds::wordsCalldata(uint256[]) [4, 5] => 2
//@ run-call: dynamicStruct((bytes)) (0x) => 0
//@ run-call: wideDynamic((uint256,bytes)) (0, 0x) => 0
//@ run-call: staticStructs((uint256,address)[]) [(5, 0x0000000000000000000000000000000000000001), (7, 0x0000000000000000000000000000000000000002)] => 14
//@ run-call-fail: 0x0d39c72c0000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0xb59b137f0000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0x616c46ff0000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0x6918e29e0000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0xb47f1a210000000000000000000000000000000000000000000000000000000000000020
//@ run-call-fail: 0xe16c853400000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0xb2bb789c00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000500000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: 0xb59b137f00000000000000000000000000000000000000000000000000000000000000200800000000000000000000000000000000000000000000000000000000000000

contract Bounds {
    struct Dynamic {
        bytes data;
    }

    struct WideDynamic {
        uint256 value;
        bytes data;
    }

    struct StaticPair {
        uint256 value;
        address account;
    }

    function bytesMemory(bytes memory value) external pure returns (uint256) {
        return value.length;
    }

    function wordsMemory(uint256[] memory value) external pure returns (uint256) {
        return value.length;
    }

    function bytesCalldata(bytes calldata value) external pure returns (uint256) {
        return value.length;
    }

    function wordsCalldata(uint256[] calldata value) external pure returns (uint256) {
        return value.length;
    }

    function dynamicStruct(Dynamic memory value) external pure returns (uint256) {
        return value.data.length;
    }

    function wideDynamic(WideDynamic memory value) external pure returns (uint256) {
        return value.data.length;
    }

    function staticStructs(StaticPair[] memory values) external pure returns (uint256) {
        return values.length + values[0].value + values[1].value;
    }
}
