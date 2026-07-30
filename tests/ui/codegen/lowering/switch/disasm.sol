//@ revisions: auto linear binary buckets dense perfect
//@[auto] compile-flags: -Zcodegen -O gas -Zdump=disasm-runtime
//@[linear] compile-flags: -Zcodegen -O gas -Zswitch-lowering=linear -Zdump=disasm-runtime
//@[binary] compile-flags: -Zcodegen -O gas -Zswitch-lowering=binary -Zdump=disasm-runtime
//@[buckets] compile-flags: -Zcodegen -O gas -Zswitch-lowering=buckets -Zdump=disasm-runtime
//@[dense] compile-flags: -Zcodegen -O gas -Zswitch-lowering=dense -Zdump=disasm-runtime
//@[perfect] compile-flags: -Zcodegen -O gas -Zswitch-lowering=perfect -Zdump=disasm-runtime

contract SwitchDisassembly {
    fallback() external {
        assembly {
            switch calldataload(0)
            case 10 {
                mstore(0, 100)
                return(0, 32)
            }
            case 12 {
                mstore(0, 101)
                return(0, 32)
            }
            case 14 {
                mstore(0, 102)
                return(0, 32)
            }
            case 16 {
                mstore(0, 103)
                return(0, 32)
            }
            case 18 {
                mstore(0, 104)
                return(0, 32)
            }
            case 20 {
                mstore(0, 105)
                return(0, 32)
            }
            case 22 {
                mstore(0, 106)
                return(0, 32)
            }
            case 24 {
                mstore(0, 107)
                return(0, 32)
            }
            default { revert(0, 0) }
        }
    }
}
