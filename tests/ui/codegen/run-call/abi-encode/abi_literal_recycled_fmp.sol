//@ codegen-matrix: standard native
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@ run-call: probe => 43981

contract C {
    function echo(bytes memory value) external pure returns (uint256) {
        return uint16(bytes2(value));
    }

    function probe() external view returns (uint256) {
        bytes memory old = new bytes(32);
        assembly {
            mstore(0x40, old)
        }
        bytes memory value = hex"1111";
        assembly {
            mstore(add(old, 32), shl(240, 0xabcd))
        }
        return this.echo(value);
    }
}
