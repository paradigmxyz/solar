//@ revisions: cancun paris
//@[cancun] compile-flags: --evm-version cancun
//@[paris] compile-flags: --evm-version paris
//@[cancun] run-call: currentBlockHash() => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[cancun] run-call: missingBlobHash() => 0x0000000000000000000000000000000000000000000000000000000000000000

contract EnvironmentHashBuiltins {
    function currentBlockHash() external view returns (bytes32) {
        return blockhash(block.number);
    }

    function missingBlobHash() external view returns (bytes32) {
        return blobhash(0);
        //~[paris]^ ERROR: codegen requires Cancun-compatible EVM for `blobhash`
        //~[paris]| HELP: compile with `--evm-version cancun` or newer
    }
}
