//@ run-call: test() => 0xa7a0d537
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_memory_v2.sol

contract AbiEncodeCallMemoryTarget {
    function something() external pure {}
}

contract AbiEncodeCallMemory {
    function something() external pure {}

    function test() external returns (bytes4) {
        function() external[2] memory pointers;
        pointers[0] = this.something;
        pointers[1] = (new AbiEncodeCallMemoryTarget()).something;
        function() external pointer = pointers[1];
        bytes memory first = abi.encodeCall(pointers[0], ());
        bytes memory second = abi.encodeCall(pointers[1], ());
        bytes memory third = abi.encodeCall(pointer, ());
        assert(first.length == 4 && second.length == 4 && third.length == 4);
        assert(bytes4(first) == bytes4(second));
        assert(bytes4(first) == bytes4(third));
        assert(bytes4(first) == pointer.selector);
        return bytes4(first);
    }
}
