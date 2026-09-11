//@ codegen-matrix: standard none_amsterdam gas_amsterdam size_amsterdam
//@[none_amsterdam] compile-flags: -Onone --evm-version amsterdam
//@[gas_amsterdam] compile-flags: -Ogas --evm-version amsterdam
//@[size_amsterdam] compile-flags: -Osize --evm-version amsterdam
//@ run-call: DynamicWriterSpills::wideEdge false => 1
//@ run-call: DynamicWriterSpills::wideEdge true => 1
//@ run-call: DynamicWriterSpills::residentOffsets false => 2457
//@ run-call: DynamicWriterSpills::residentOffsets true => 2457
//@ run-call: DynamicWriterSpills::run => 1
//@ run-call: DynamicWriterSpills::storeDeep => 1
//@ run-call: FrameArgsAcrossCopy::run false => 44
//@ run-call: FrameArgsAcrossCopy::run true => 44

contract DynamicWriterSpills {
    function payload() external pure {
        assembly {
            return(0, 0x600)
        }
    }

    function run() external view returns (uint256 result) {
        assembly {
            let a00 := gas()
            let a01 := gas()
            let a02 := gas()
            let a03 := gas()
            let a04 := gas()
            let a05 := gas()
            let a06 := gas()
            let a07 := gas()
            let a08 := gas()
            let a09 := gas()
            let a10 := gas()
            let a11 := gas()
            let a12 := gas()
            let a13 := gas()
            let a14 := gas()
            let a15 := gas()
            let a16 := gas()
            let a17 := gas()
            let a18 := gas()
            let a19 := gas()
            mstore(0, 0xa878f858)
            if iszero(staticcall(gas(), address(), 0x1c, 4, 0, 0)) { revert(0, 0) }
            returndatacopy(0, 0, returndatasize())
            result := iszero(
                iszero(
                    and(
                        and(and(and(a00, a01), and(a02, a03)), and(and(a04, a05), and(a06, a07))),
                        and(
                            and(and(and(a08, a09), and(a10, a11)), and(and(a12, a13), and(a14, a15))),
                            and(and(a16, a17), and(a18, a19))
                        )
                    )
                )
            )
        }
    }

    function storeDeep() external view returns (uint256 result) {
        assembly {
            let a00 := gas()
            let a01 := gas()
            let a02 := gas()
            let a03 := gas()
            let a04 := gas()
            let a05 := gas()
            let a06 := gas()
            let a07 := gas()
            let a08 := gas()
            let a09 := gas()
            let a10 := gas()
            let a11 := gas()
            let a12 := gas()
            let a13 := gas()
            let a14 := gas()
            let a15 := gas()
            let a16 := gas()
            let a17 := gas()
            let a18 := gas()
            let a19 := gas()
            mstore(0x0, a00)
            mstore(0x20, a01)
            mstore(0x40, a02)
            mstore(0x60, a03)
            mstore(0x80, a04)
            mstore(0xa0, a05)
            mstore(0xc0, a06)
            mstore(0xe0, a07)
            mstore(0x100, a08)
            mstore(0x120, a09)
            mstore(0x140, a10)
            mstore(0x160, a11)
            mstore(0x180, a12)
            mstore(0x1a0, a13)
            mstore(0x1c0, a14)
            mstore(0x1e0, a15)
            mstore(0x200, a16)
            mstore(0x220, a17)
            mstore(0x240, a18)
            mstore(0x260, a19)
            result := and(iszero(iszero(keccak256(0, 0x280))), iszero(mload(0x280)))
        }
    }
    function residentOffsets(bool branch) external returns (uint256 result) {
        assembly {
            mstore(0x40, 0x80)
            let base := add(sload(0), 128)
            let a1 := add(base, 1)
            let a2 := add(base, 2)
            let a3 := add(base, 3)
            let a4 := add(base, 4)
            let a5 := add(base, 5)
            let a6 := add(base, 6)
            let a7 := add(base, 7)
            let a8 := add(base, 8)
            let a9 := add(base, 9)
            let a10 := add(base, 10)
            let a11 := add(base, 11)
            let a12 := add(base, 12)
            let a13 := add(base, 13)
            let a14 := add(base, 14)
            let a15 := add(base, 15)
            let a16 := add(base, 16)
            let a17 := add(base, 17)
            if branch { sstore(1, 1) }
            result := base
            result := add(result, a1)
            result := add(result, a2)
            result := add(result, a3)
            result := add(result, a4)
            result := add(result, a5)
            result := add(result, a6)
            result := add(result, a7)
            result := add(result, a8)
            result := add(result, a9)
            result := add(result, a10)
            result := add(result, a11)
            result := add(result, a12)
            result := add(result, a13)
            result := add(result, a14)
            result := add(result, a15)
            result := add(result, a16)
            result := add(result, a17)
        }
    }
    function wideEdge(bool branch) external returns (uint256 result) {
        assembly {
            function bump(word) -> changed { changed := add(word, 1) }
            mstore(0, gas())
            mstore(32, gas())
            mstore(64, gas())
            mstore(96, gas())
            mstore(128, gas())
            mstore(160, gas())
            mstore(192, gas())
            mstore(224, gas())
            mstore(256, gas())
            mstore(288, gas())
            mstore(320, gas())
            mstore(352, gas())
            mstore(384, gas())
            mstore(416, gas())
            mstore(448, gas())
            mstore(480, gas())
            mstore(512, gas())
            mstore(544, gas())
            mstore(576, gas())
            mstore(608, gas())
            let v0 := mload(0)
            let v1 := mload(32)
            let v2 := mload(64)
            let v3 := mload(96)
            let v4 := mload(128)
            let v5 := mload(160)
            let v6 := mload(192)
            let v7 := mload(224)
            let v8 := mload(256)
            let v9 := mload(288)
            let v10 := mload(320)
            let v11 := mload(352)
            let v12 := mload(384)
            let v13 := mload(416)
            let v14 := mload(448)
            let v15 := mload(480)
            let v16 := mload(512)
            let v17 := mload(544)
            let v18 := mload(576)
            let v19 := mload(608)
            if branch { sstore(1, 1) }
            v19 := bump(v19)
            v18 := bump(v18)
            mstore(608, v19)
            mstore(576, v18)
            mstore(4096, v0)
            mstore(4128, v1)
            mstore(4160, v2)
            mstore(4192, v3)
            mstore(4224, v4)
            mstore(4256, v5)
            mstore(4288, v6)
            mstore(4320, v7)
            mstore(4352, v8)
            mstore(4384, v9)
            mstore(4416, v10)
            mstore(4448, v11)
            mstore(4480, v12)
            mstore(4512, v13)
            mstore(4544, v14)
            mstore(4576, v15)
            mstore(4608, v16)
            mstore(4640, v17)
            mstore(4672, v18)
            mstore(4704, v19)
            result := and(eq(keccak256(0, 640), keccak256(4096, 640)), iszero(mload(640)))
        }
    }
}

contract FrameArgsAcrossCopy {
    struct Buffer { bytes data; }

    function run(bool branch) external pure returns (uint256) {
        return preserve(Buffer(new bytes(32)), 5, branch);
    }

    function seed() internal pure returns (uint256) {
        return 7;
    }

    function preserve(Buffer memory buffer, uint256 value, bool branch)
        internal pure returns (uint256 result)
    {
        uint256 initial = seed();
        assembly ("memory-safe") {
            let data := mload(buffer)
            if branch {
                calldatacopy(add(data, 32), calldatasize(), mload(data))
            }
            result := add(initial, add(value, mload(data)))
        }
    }
}
