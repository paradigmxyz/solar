//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: calldataBytes 0x68656c6c6f => 1
//@ run-call: calldataString "hello" => 1
//@ run-call: calldataSlice 0x68656c6c6f => 1
//@ run-call: memoryBytes 0x68656c6c6f => 1
//@ run-call: calldataStruct (1, 2) => 1
//@ run-call: memoryStruct 1, 2 => 1
//@ run-call: signatureCalldata "transfer(address,uint256)" => 1
//@ run-call: signatureMemory "transfer(address,uint256)" => 1
//@ run-call: erc7201Memory "example.main" => 1

// A hash input that the hash reads at once is staged at the free memory pointer without
// reserving it, as solc does, so no input below changes what a later `mload(0x40)` sees.
contract StagedHashInputs {
    struct Pair {
        uint256 first;
        uint256 second;
    }

    event Bytes(bytes indexed value);
    event Text(string indexed value);
    event Static(Pair indexed value);

    function calldataBytes(bytes calldata value) external returns (uint256 ok) {
        uint256 before = freePointer();
        emit Bytes(value);
        ok = freePointer() == before ? 1 : 0;
    }

    function calldataString(string calldata value) external returns (uint256 ok) {
        uint256 before = freePointer();
        emit Text(value);
        ok = freePointer() == before ? 1 : 0;
    }

    function calldataSlice(bytes calldata value) external returns (uint256 ok) {
        uint256 before = freePointer();
        emit Bytes(value[1:3]);
        ok = freePointer() == before ? 1 : 0;
    }

    function memoryBytes(bytes memory value) external returns (uint256 ok) {
        uint256 before = freePointer();
        emit Bytes(value);
        ok = freePointer() == before ? 1 : 0;
    }

    function calldataStruct(Pair calldata value) external returns (uint256 ok) {
        uint256 before = freePointer();
        emit Static(value);
        ok = freePointer() == before ? 1 : 0;
    }

    function memoryStruct(uint256 first, uint256 second) external returns (uint256 ok) {
        Pair memory value = Pair(first, second);
        uint256 before = freePointer();
        emit Static(value);
        ok = freePointer() == before ? 1 : 0;
    }

    // The encoding owns 32 + 64 bytes: the hash of the signature adds nothing.
    function signatureCalldata(string calldata signature) external pure returns (uint256 ok) {
        uint256 before = freePointer();
        bytes memory data = abi.encodeWithSignature(signature, 1);
        ok = freePointer() - before == 96 && bytes4(data) == bytes4(keccak256(bytes(signature)))
            ? 1
            : 0;
    }

    function signatureMemory(string memory signature) external pure returns (uint256 ok) {
        uint256 before = freePointer();
        bytes memory data = abi.encodeWithSignature(signature, 1);
        ok = freePointer() - before == 96 && bytes4(data) == bytes4(keccak256(bytes(signature)))
            ? 1
            : 0;
    }

    function erc7201Memory(string memory id) external pure returns (uint256 ok) {
        uint256 before = freePointer();
        uint256 slot = erc7201(id);
        ok = freePointer() == before && slot == expectedSlot() ? 1 : 0;
    }

    function expectedSlot() private pure returns (uint256) {
        return uint256(keccak256(abi.encode(uint256(keccak256("example.main")) - 1))) & ~uint256(0xff);
    }

    function freePointer() private pure returns (uint256 pointer) {
        assembly {
            pointer := mload(0x40)
        }
    }
}
