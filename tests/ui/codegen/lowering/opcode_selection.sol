//@ compile-flags: -O none --evm-version amsterdam -Zdump=evm-ir-runtime
//@ filecheck:

// Each MIR operation that lowers to a single EVM opcode, reached through its
// inline assembly builtin. Unoptimized code keeps the source order.
contract OpcodeSelection {
    // CHECK-LABEL: (runtime) ===
    function all() external {
        assembly {
            // CHECK: {{^ +}}add{{$}}
            mstore(0, add(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}sub{{$}}
            mstore(32, sub(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}mul{{$}}
            mstore(64, mul(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}div{{$}}
            mstore(96, div(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}sdiv{{$}}
            mstore(128, sdiv(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}mod{{$}}
            mstore(160, mod(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}smod{{$}}
            mstore(192, smod(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}exp{{$}}
            mstore(224, exp(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}addmod{{$}}
            mstore(256, addmod(calldataload(4), calldataload(36), calldataload(68)))
            // CHECK: {{^ +}}mulmod{{$}}
            mstore(288, mulmod(calldataload(4), calldataload(36), calldataload(68)))
            // CHECK: {{^ +}}and{{$}}
            mstore(320, and(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}or{{$}}
            mstore(352, or(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}xor{{$}}
            mstore(384, xor(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}not{{$}}
            mstore(416, not(calldataload(4)))
            // CHECK: {{^ +}}clz{{$}}
            mstore(448, clz(calldataload(4)))
            // CHECK: {{^ +}}shl{{$}}
            mstore(480, shl(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}shr{{$}}
            mstore(512, shr(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}sar{{$}}
            mstore(544, sar(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}byte{{$}}
            mstore(576, byte(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}lt{{$}}
            mstore(608, lt(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}gt{{$}}
            mstore(640, gt(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}slt{{$}}
            mstore(672, slt(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}sgt{{$}}
            mstore(704, sgt(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}eq{{$}}
            mstore(736, eq(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}mload{{$}}
            mstore(768, mload(calldataload(4)))
            // CHECK: {{^ +}}mstore{{$}}
            mstore(calldataload(4), calldataload(36))
            // CHECK: {{^ +}}mstore8{{$}}
            mstore8(calldataload(4), calldataload(36))
            // CHECK: {{^ +}}msize{{$}}
            mstore(864, msize())
            // CHECK: {{^ +}}mcopy{{$}}
            mcopy(calldataload(4), calldataload(36), calldataload(68))
            // CHECK: {{^ +}}sload{{$}}
            mstore(928, sload(calldataload(4)))
            // CHECK: {{^ +}}sstore{{$}}
            sstore(calldataload(4), calldataload(36))
            // CHECK: {{^ +}}tload{{$}}
            mstore(992, tload(calldataload(4)))
            // CHECK: {{^ +}}tstore{{$}}
            tstore(calldataload(4), calldataload(36))
            // CHECK: {{^ +}}calldataload{{$}}
            mstore(1056, calldataload(calldataload(4)))
            // CHECK: {{^ +}}calldatacopy{{$}}
            calldatacopy(calldataload(4), calldataload(36), calldataload(68))
            // CHECK: {{^ +}}calldatasize{{$}}
            mstore(1120, calldatasize())
            // CHECK: {{^ +}}codesize{{$}}
            mstore(1152, codesize())
            // CHECK: {{^ +}}codecopy{{$}}
            codecopy(calldataload(4), calldataload(36), calldataload(68))
            // CHECK: {{^ +}}extcodesize{{$}}
            mstore(1216, extcodesize(calldataload(4)))
            // CHECK: {{^ +}}extcodecopy{{$}}
            extcodecopy(calldataload(4), calldataload(36), calldataload(68), calldataload(100))
            // CHECK: {{^ +}}extcodehash{{$}}
            mstore(1280, extcodehash(calldataload(4)))
            // CHECK: {{^ +}}returndatasize{{$}}
            mstore(1312, returndatasize())
            // CHECK: {{^ +}}returndatacopy{{$}}
            returndatacopy(calldataload(4), calldataload(36), calldataload(68))
            // CHECK: {{^ +}}caller{{$}}
            mstore(1376, caller())
            // CHECK: {{^ +}}callvalue{{$}}
            mstore(1408, callvalue())
            // CHECK: {{^ +}}origin{{$}}
            mstore(1440, origin())
            // CHECK: {{^ +}}gasprice{{$}}
            mstore(1472, gasprice())
            // CHECK: {{^ +}}blockhash{{$}}
            mstore(1504, blockhash(calldataload(4)))
            // CHECK: {{^ +}}coinbase{{$}}
            mstore(1536, coinbase())
            // CHECK: {{^ +}}timestamp{{$}}
            mstore(1568, timestamp())
            // CHECK: {{^ +}}number{{$}}
            mstore(1600, number())
            // CHECK: {{^ +}}prevrandao{{$}}
            mstore(1632, prevrandao())
            // CHECK: {{^ +}}gaslimit{{$}}
            mstore(1664, gaslimit())
            // CHECK: {{^ +}}slotnum{{$}}
            mstore(1696, slotnum())
            // CHECK: {{^ +}}chainid{{$}}
            mstore(1728, chainid())
            // CHECK: {{^ +}}address{{$}}
            mstore(1760, address())
            // CHECK: {{^ +}}balance{{$}}
            mstore(1792, balance(calldataload(4)))
            // CHECK: {{^ +}}selfbalance{{$}}
            mstore(1824, selfbalance())
            // CHECK: {{^ +}}gas{{$}}
            mstore(1856, gas())
            // CHECK: {{^ +}}basefee{{$}}
            mstore(1888, basefee())
            // CHECK: {{^ +}}blobbasefee{{$}}
            mstore(1920, blobbasefee())
            // CHECK: {{^ +}}blobhash{{$}}
            mstore(1952, blobhash(calldataload(4)))
            // CHECK: {{^ +}}keccak256{{$}}
            mstore(1984, keccak256(calldataload(4), calldataload(36)))
            // CHECK: {{^ +}}create{{$}}
            mstore(2016, create(calldataload(4), calldataload(36), calldataload(68)))
            // CHECK: {{^ +}}create2{{$}}
            mstore(2048, create2(calldataload(4), calldataload(36), calldataload(68), calldataload(100)))
            // CHECK: {{^ +}}log0{{$}}
            log0(calldataload(4), calldataload(36))
            // CHECK: {{^ +}}log1{{$}}
            log1(calldataload(4), calldataload(36), calldataload(68))
            // CHECK: {{^ +}}log2{{$}}
            log2(calldataload(4), calldataload(36), calldataload(68), calldataload(100))
            // CHECK: {{^ +}}log3{{$}}
            log3(calldataload(4), calldataload(36), calldataload(68), calldataload(100), calldataload(132))
            // CHECK: {{^ +}}log4{{$}}
            log4(calldataload(4), calldataload(36), calldataload(68), calldataload(100), calldataload(132), calldataload(164))
            // CHECK: {{^ +}}signextend{{$}}
            mstore(2240, signextend(calldataload(4), calldataload(36)))
        }
    }
}
