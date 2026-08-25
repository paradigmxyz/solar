//@ revisions: osaka prague
//@[osaka] compile-flags: --evm-version osaka
//@[prague] compile-flags: --evm-version prague
//@[osaka] run-call: leadingZeros 0 => 256
//@[osaka] run-call: leadingZeros 1 => 255
//@[osaka] run-call: leadingZeros 0xff => 248
//@[osaka] run-call: leadingZeros 0x4000000000000000000000000000000000000000000000000000000000000000 => 1
//@[osaka] run-call: invokeCallCode => 42

contract YulClzCallCode {
    function leadingZeros(uint256 value) external pure returns (uint256 result) {
        assembly {
            result := clz(value)
            //~[prague]^ ERROR: codegen requires Osaka-compatible EVM for `clz`
            //~[prague]| HELP: compile with `--evm-version osaka` or newer
        }
    }

    function target() external pure returns (uint256) {
        return 42;
    }

    function invokeCallCode() external returns (uint256 result) {
        bytes4 selector = this.target.selector;
        assembly {
            mstore(0, selector)
            if iszero(callcode(gas(), address(), 0, 0, 4, 0, 32)) {
                revert(0, 0)
            }
            result := mload(0)
        }
    }
}
