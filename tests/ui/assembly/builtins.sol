// Test all categories of Yul EVM builtins

contract YulBuiltins {
    function arithmetic() public pure returns (uint256) {
        assembly {
            let a := 10
            let b := 3

            let r1 := add(a, b)
            let r2 := sub(a, b)
            let r3 := mul(a, b)
            let r4 := div(a, b)
            let r5 := sdiv(a, b)
            let r6 := mod(a, b)
            let r7 := smod(a, b)
            let r8 := exp(a, b)
            let r9 := addmod(a, b, 7)
            let r10 := mulmod(a, b, 7)

            mstore(0, r1)
            return(0, 32)
        }
    }

    function comparison() public pure returns (uint256) {
        assembly {
            let a := 10
            let b := 20

            let r1 := lt(a, b)
            let r2 := gt(a, b)
            let r3 := slt(a, b)
            let r4 := sgt(a, b)
            let r5 := eq(a, b)
            let r6 := iszero(a)

            mstore(0, r1)
            return(0, 32)
        }
    }

    function bitwise() public pure returns (uint256) {
        assembly {
            let a := 0xff
            let b := 0x0f

            let r1 := and(a, b)
            let r2 := or(a, b)
            let r3 := xor(a, b)
            let r4 := not(a)
            let r5 := shl(4, a)
            let r6 := shr(4, a)
            let r7 := sar(4, a)
            let r8 := byte(31, a)
            let r9 := signextend(0, a)

            mstore(0, r1)
            return(0, 32)
        }
    }

    function memoryOps() public pure returns (uint256) {
        assembly {
            mstore(0x40, 0x80)
            mstore8(0x80, 0x42)
            let val := mload(0x40)
            let size := msize()
            mcopy(0x100, 0x40, 32)

            mstore(0, val)
            return(0, 32)
        }
    }

    function storageOps() public returns (uint256) {
        assembly {
            sstore(0, 42)
            let val := sload(0)
            tstore(1, 100)
            let tval := tload(1)

            mstore(0, val)
            return(0, 32)
        }
    }

    function calldataOps() public pure returns (uint256) {
        assembly {
            let size := calldatasize()
            let val := calldataload(0)
            calldatacopy(0x80, 0, size)

            mstore(0, val)
            return(0, 32)
        }
    }

    function environmentOps() public view returns (address) {
        assembly {
            let c := caller()
            let v := callvalue()
            let o := origin()
            let g := gas()
            let p := gasprice()
            let a := address()
            let b := balance(a)
            let sb := selfbalance()

            mstore(0, c)
            return(0, 32)
        }
    }

    function blockOps() public view returns (uint256) {
        assembly {
            let bh := blockhash(sub(number(), 1))
            let cb := coinbase()
            let ts := timestamp()
            let num := number()
            let pr := prevrandao()
            let gl := gaslimit()
            let cid := chainid()
            let bf := basefee()
            let bbf := blobbasefee()

            mstore(0, num)
            return(0, 32)
        }
    }

    function hashOps() public pure returns (bytes32) {
        assembly {
            mstore(0, 0x1234)
            let h := keccak256(0, 32)
            mstore(0, h)
            return(0, 32)
        }
    }

    function codeOps() public view returns (uint256) {
        assembly {
            let size := codesize()
            codecopy(0x80, 0, size)
            let extSize := extcodesize(address())
            extcodecopy(address(), 0x100, 0, extSize)
            let hash := extcodehash(address())

            mstore(0, size)
            return(0, 32)
        }
    }

    function returnDataOps() public view returns (uint256) {
        assembly {
            pop(staticcall(gas(), address(), 0, 0, 0, 0))
            let size := returndatasize()
            returndatacopy(0x80, 0, size)

            mstore(0, size)
            return(0, 32)
        }
    }

    function logOps() public {
        assembly {
            mstore(0, 0x1234)
            log0(0, 32)
            log1(0, 32, 0xabc)
            log2(0, 32, 0xabc, 0xdef)
            log3(0, 32, 0xabc, 0xdef, 0x123)
            log4(0, 32, 0xabc, 0xdef, 0x123, 0x456)
        }
    }

    function controlOps() public pure {
        assembly {
            if 0 {
                invalid()
            }
            if 0 {
                stop()
            }
        }
    }

    function popOp() public pure {
        assembly {
            pop(add(1, 2))
        }
    }
}
