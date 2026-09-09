//@ codegen-matrix: standard none_debug gas_debug size_debug
//@[none_debug] compile-flags: -Onone --emit=abi,bin,ethdebug
//@[gas_debug] compile-flags: -Ogas --emit=abi,bin,ethdebug
//@[size_debug] compile-flags: -Osize --emit=abi,bin,ethdebug
//~[none,gas,size,none_debug,gas_debug,size_debug]? ERROR: codegen cannot recover deep forwarding values without changing `msize`

// A callee's scratch allocation remains visible to a later sibling call.
contract ForwardingMsize {
    fallback() external {
        recover();
        uint256 size = readSize();
        assembly {
            mstore(0, size)
            return(0, 32)
        }
    }

    function readSize() internal pure returns (uint256 size) {
        assembly { size := msize() }
    }

    function recover() internal {
        assembly {
            calldatacopy(0, 0, calldatasize())
            let x0 := sload(0)
            let x1 := sload(1)
            let x2 := sload(2)
            let x3 := sload(3)
            let x4 := sload(4)
            let x5 := sload(5)
            let x6 := sload(6)
            let x7 := sload(7)
            let x8 := sload(8)
            let x9 := sload(9)
            let x10 := sload(10)
            let x11 := sload(11)
            let x12 := sload(12)
            let x13 := sload(13)
            let x14 := sload(14)
            let x15 := sload(15)
            let x16 := sload(16)
            let x17 := sload(17)
            sstore(0, x0)
            sstore(1, x1)
            sstore(2, x2)
            sstore(3, x3)
            sstore(4, x4)
            sstore(5, x5)
            sstore(6, x6)
            sstore(7, x7)
            sstore(8, x8)
            sstore(9, x9)
            sstore(10, x10)
            sstore(11, x11)
            sstore(12, x12)
            sstore(13, x13)
            sstore(14, x14)
            sstore(15, x15)
            sstore(16, x16)
            sstore(17, x17)
        }
    }
}
