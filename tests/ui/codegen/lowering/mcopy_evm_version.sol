//@compile-flags: -O none --evm-version shanghai -Zdump=mir

contract McopyEvmVersion {
    function copy() external pure {
        assembly {
            mcopy(0x80, 0xa0, 0x20)
            //~^ ERROR: Yul builtin `mcopy` requires Cancun-compatible EVM
            //~| HELP: compile with `--evm-version cancun` or newer
        }
    }
}
