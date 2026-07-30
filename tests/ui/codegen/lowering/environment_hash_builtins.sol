//@ run-call: currentBlockHash() => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: missingBlobHash() => 0x0000000000000000000000000000000000000000000000000000000000000000

contract EnvironmentHashBuiltins {
    function currentBlockHash() external view returns (bytes32) {
        return blockhash(block.number);
    }

    function missingBlobHash() external view returns (bytes32) {
        return blobhash(0);
    }
}
