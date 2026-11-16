//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test => 15
//@ run-call: storageEncoded => 0x33d858100000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000033132330000000000000000000000000000000000000000000000000000000000
//@ run-call: localEncoded => 0x33d858100000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000033132330000000000000000000000000000000000000000000000000000000000
//@ run-call: returnedEncoded => 0x33d858100000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000033132330000000000000000000000000000000000000000000000000000000000
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_call_is_consistent_v2.sol

contract AbiEncodeCallConsistent {
    bool sideEffectRan;
    function(uint256, string memory) external fPointer;

    function fExternal(uint256, string memory) external {}

    function getExternalFunctionPointer()
        public
        returns (function(uint256, string memory) external)
    {
        sideEffectRan = true;
        return this.fExternal;
    }

    function test() public returns (uint8) {
        bytes memory expected = abi.encodeWithSignature("fExternal(uint256,string)", 1, "123");

        fPointer = this.fExternal;
        bytes memory storagePointer = abi.encodeCall(fPointer, (1, "123"));

        function(uint256, string memory) external localPointer = this.fExternal;
        bytes memory local = abi.encodeCall(localPointer, (1, "123"));

        bytes memory returned = abi.encodeCall(getExternalFunctionPointer(), (1, "123"));

        uint8 result;
        if (keccak256(expected) == keccak256(storagePointer)) result |= 1;
        if (keccak256(expected) == keccak256(local)) result |= 2;
        if (keccak256(expected) == keccak256(returned)) result |= 4;
        if (sideEffectRan) result |= 8;
        return result;
    }

    function storageEncoded() public returns (bytes memory) {
        fPointer = this.fExternal;
        return abi.encodeCall(fPointer, (1, "123"));
    }

    function localEncoded() public view returns (bytes memory) {
        function(uint256, string memory) external localPointer = this.fExternal;
        return abi.encodeCall(localPointer, (1, "123"));
    }

    function returnedEncoded() public returns (bytes memory) {
        return abi.encodeCall(getExternalFunctionPointer(), (1, "123"));
    }
}
