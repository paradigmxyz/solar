//@ run-call: hash((uint256[])[]) [([17, 42, 23])] => 0xf57b3c600f9ec7a5d46d1bf0c4393033dd73c72ac953e8c96840916a10153eca
//@ run-call: hash((uint256[])[]) [([17, 42, 23]), ([51, 72])] => 0x694011fc78ca93e7f2439364ce276a3792bcc300d70c824716935888f0127d4c
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_struct_dynamic_v2.sol

pragma abicoder v2;

contract AbiCalldataStructDynamic {
    struct Entry {
        uint256[] values;
    }

    function hash(Entry[] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encode(values));
    }
}
