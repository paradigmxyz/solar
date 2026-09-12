//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: memoryLoop => 1
//@ run-call: test => 0xa7a0d537
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_memory_v2.sol

contract AbiEncodeCallMemoryTarget {
    function something() external pure {}
}

contract AbiEncodeCallMemory {
    function something() external pure {}

    function accept(bytes calldata data, uint256[] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encode(data, values));
    }

    function next() external pure returns (bytes memory data) {
        data = new bytes(480);
        assembly { mstore(add(data, 32), 1) }
    }

    function memoryLoop() external returns (uint256) {
        bytes memory data;
        uint256[] memory values = new uint256[](2);
        values[0] = 42;
        values[1] = 99;
        for (uint256 i; i < 2; ++i) {
            bytes memory encoded = abi.encodeCall(this.accept, (data, values));
            (bool ok, bytes memory result) = address(this).call(encoded);
            require(ok);
            require(abi.decode(result, (bytes32)) == keccak256(abi.encode(data, values)));
            data = this.next();
            values = new uint256[](3);
            values[2] = 123;
        }
        return 1;
    }

    // CHECK-LABEL: fn @test
    // CHECK-NOT: phi
    // CHECK: returndata
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
