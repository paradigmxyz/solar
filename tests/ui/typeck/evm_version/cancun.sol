//@ revisions: shanghai cancun
//@[shanghai] compile-flags: --evm-version shanghai
//@[cancun] compile-flags: --evm-version cancun
// ported-from: test/libsolidity/syntaxTests/types/magic_block_blobbasefee_error.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/blobhash_not_declared_pre_cancun.sol
// ported-from: test/libsolidity/syntaxTests/dataLocations/transient_storage_variable_pre_cancun.sol

contract C {
    uint256 transient value;
    //~[shanghai]^ ERROR: transient storage requires Cancun-compatible EVM

    function blobHash() public view returns (bytes32) {
        return blobhash(0);
        //~[shanghai]^ ERROR: builtin `blobhash` requires Cancun-compatible EVM
    }

    function blobBaseFee() public view returns (uint256) {
        return block.blobbasefee;
        //~[shanghai]^ ERROR: builtin `blobbasefee` requires Cancun-compatible EVM
    }
}
