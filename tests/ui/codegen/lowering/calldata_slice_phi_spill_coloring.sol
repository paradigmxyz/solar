//@ compile-flags: -Ogas
//@ run-call: select 0x010203040506, false => 2, 5, 502
//@ run-call: select 0x010203040506, true => 4, 3, 502

contract CalldataSlicePhiSpillColoring {
    function select(bytes calldata data, bool trim)
        external
        pure
        returns (uint256 first, uint256 length, uint256 marker)
    {
        bytes calldata base = data[1:];
        bytes calldata chosen = base;
        marker = base.length * 100 + uint8(base[0]);
        if (trim) chosen = base[2:];
        first = uint8(chosen[0]);
        length = chosen.length;
    }
}
