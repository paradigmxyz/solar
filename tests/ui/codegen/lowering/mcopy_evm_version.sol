//@compile-flags: -Zcodegen --evm-version shanghai -Zdump=mir

contract McopyEvmVersion {
    function copy() external pure {
        assembly {
            mcopy(0x80, 0xa0, 0x20)
            //~^ ERROR: codegen requires Cancun-compatible EVM for memory copy
            //~| HELP: compile with `--evm-version cancun` or newer
        }
    }
}
