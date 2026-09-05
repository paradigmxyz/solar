//@ codegen-matrix: standard
//@ run-call: observe false, 7 => 4096, 4103
//@ run-call: observe false, 255 => 4096, 4351
//@ run-call: observe true, 7 => 4096, 1792
//@ run-call: observe true, 255 => 4096, 65280

// Partial writes that overlap the free-memory-pointer word invalidate its current value.
// The earlier observation remains live across the join and must retain its original value.
contract FmpPartialWrites {
    function observe(bool wordWrite, uint8 value)
        external
        pure
        returns (uint256 beforeWrite, uint256 afterWrite)
    {
        assembly {
            let saved := mload(0x40)
            mstore(0x40, 0x1000)
            beforeWrite := mload(0x40)
            switch wordWrite
            case 0 { mstore8(0x5f, value) }
            default { mstore(0x3f, value) }
            afterWrite := mload(0x40)
            mstore(0x40, saved)
        }
    }
}
