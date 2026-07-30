//@ revisions: cancun paris
//@[cancun] compile-flags: --evm-version cancun
//@[paris] compile-flags: --evm-version paris
//@[cancun] run-call: currentBlockHash() => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[cancun] run-call: missingBlobHash() => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[cancun] run-call: missingYulBlobHash() => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[cancun] run-call: currentBlobBaseFee() => 1
//@[cancun] run-call: currentYulBlobBaseFee() => 1

contract EnvironmentHashBuiltins {
    function currentBlockHash() external view returns (bytes32) {
        return blockhash(block.number);
    }

    function missingBlobHash() external view returns (bytes32) {
        return blobhash(0);
        //~[paris]^ ERROR: codegen requires Cancun-compatible EVM for `blobhash`
        //~[paris]| HELP: compile with `--evm-version cancun` or newer
    }

    function missingYulBlobHash() external view returns (bytes32 result) {
        assembly {
            result := blobhash(0)
            //~[paris]^ ERROR: codegen requires Cancun-compatible EVM for `blobhash`
            //~[paris]| HELP: compile with `--evm-version cancun` or newer
        }
    }

    function currentBlobBaseFee() external view returns (uint256) {
        return block.blobbasefee;
        //~[paris]^ ERROR: codegen requires Cancun-compatible EVM for `block.blobbasefee`
        //~[paris]| HELP: compile with `--evm-version cancun` or newer
    }

    function currentYulBlobBaseFee() external view returns (uint256 result) {
        assembly {
            result := blobbasefee()
            //~[paris]^ ERROR: codegen requires Cancun-compatible EVM for `blobbasefee`
            //~[paris]| HELP: compile with `--evm-version cancun` or newer
        }
    }
}
