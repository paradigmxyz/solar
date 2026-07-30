//@ revisions: linear_gas linear_size binary_gas binary_size buckets_gas buckets_size dense_gas dense_size perfect_gas perfect_size
//@[linear_gas] compile-flags: -Zcodegen -O gas -Zswitch-lowering=linear -Zdump=disasm-runtime
//@[linear_size] compile-flags: -Zcodegen -O size -Zswitch-lowering=linear -Zdump=disasm-runtime
//@[binary_gas] compile-flags: -Zcodegen -O gas -Zswitch-lowering=binary -Zdump=disasm-runtime
//@[binary_size] compile-flags: -Zcodegen -O size -Zswitch-lowering=binary -Zdump=disasm-runtime
//@[buckets_gas] compile-flags: -Zcodegen -O gas -Zswitch-lowering=buckets -Zdump=disasm-runtime
//@[buckets_size] compile-flags: -Zcodegen -O size -Zswitch-lowering=buckets -Zdump=disasm-runtime
//@[dense_gas] compile-flags: -Zcodegen -O gas -Zswitch-lowering=dense -Zdump=disasm-runtime
//@[dense_size] compile-flags: -Zcodegen -O size -Zswitch-lowering=dense -Zdump=disasm-runtime
//@[perfect_gas] compile-flags: -Zcodegen -O gas -Zswitch-lowering=perfect -Zdump=disasm-runtime
//@[perfect_size] compile-flags: -Zcodegen -O size -Zswitch-lowering=perfect -Zdump=disasm-runtime

contract SwitchLowerings {
    fallback() external {
        assembly {
            switch calldataload(0)
            case 8 {
                mstore(0, 100)
                return(0, 32)
            }
            case 16 {
                mstore(0, 101)
                return(0, 32)
            }
            case 24 {
                mstore(0, 102)
                return(0, 32)
            }
            case 32 {
                mstore(0, 103)
                return(0, 32)
            }
            case 40 {
                mstore(0, 104)
                return(0, 32)
            }
            case 48 {
                mstore(0, 105)
                return(0, 32)
            }
            case 56 {
                mstore(0, 106)
                return(0, 32)
            }
            case 64 {
                mstore(0, 107)
                return(0, 32)
            }
            default { revert(0, 0) }
        }
    }
}
