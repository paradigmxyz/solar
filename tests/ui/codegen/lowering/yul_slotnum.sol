//@ revisions: amsterdam osaka
//@[amsterdam] compile-flags: --evm-version amsterdam -Zdump=evm-ir-runtime
//@[osaka] compile-flags: --evm-version osaka

contract YulSlotnum {
    function currentSlot() external view returns (uint256 result) {
        assembly {
            result := slotnum()
            //~[osaka]^ ERROR: Yul builtin `slotnum` requires Amsterdam-compatible EVM
            //~[osaka]| HELP: compile with `--evm-version amsterdam` or newer
        }
    }
}
