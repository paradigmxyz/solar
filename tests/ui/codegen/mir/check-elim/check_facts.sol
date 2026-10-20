//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: siftDown [1, 5, 3, 7], 3, 2 => [1, 2, 3, 5]
//@ run-call: siftDown [1, 5, 3, 7], 3, 0 => [0, 1, 3, 5]
//@ run-call: siftDown [1, 5, 3, 7], 0, 9 => [9, 5, 3, 7]
//@ run-call-fail: siftDown [1, 5], 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001
//@ run-call: keepsCheck [1, 5, 3, 7], 3, 2 => [1, 2, 3, 5]
//@ run-call-fail: keepsCheck [1, 5], 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: afterAllocation 3 => 6
//@ run-call: afterAllocation 0 => 0

// A passing check states its condition for the code after it, so the hole the
// assertion keeps below the length bounds every position the loop visits: a
// position only ever falls, to its parent `(pos - 1) >> 1`, which a checked
// difference keeps below `pos`. Neither the parent read nor the writes check
// their index.
// OPT-LABEL: fn @siftDown()
// OPT-NOT: tail_call @revert_stub
// OPT-NOT: mstore 32, 50
//
// Without the assertion, the start position is unbounded and the writes keep
// their checks.
// OPT-LABEL: fn @keepsCheck()
// OPT: mstore 32, 50
//
// An allocation's size checks bound its length, `(m + 1) << 5 >> 5 == m + 1`,
// so the doubled length cannot overflow.
// OPT-LABEL: fn @afterAllocation()
// OPT-NOT: tail_call @revert_stub
// OPT-NOT: mstore 32, 17
// OPT-LABEL: fn @decode_calldata_type
contract CheckFacts {
    function siftDown(uint256[] memory data, uint256 pos, uint256 value)
        external
        pure
        returns (uint256[] memory)
    {
        assert(pos < data.length);
        while (pos != 0) {
            uint256 parentPos = (pos - 1) >> 1;
            uint256 parent = data[parentPos];
            if (value >= parent) break;
            data[pos] = parent;
            pos = parentPos;
        }
        data[pos] = value;
        return data;
    }

    function keepsCheck(uint256[] memory data, uint256 pos, uint256 value)
        external
        pure
        returns (uint256[] memory)
    {
        while (pos != 0) {
            uint256 parentPos = (pos - 1) >> 1;
            uint256 parent = data[parentPos];
            if (value >= parent) break;
            data[pos] = parent;
            pos = parentPos;
        }
        data[pos] = value;
        return data;
    }

    function afterAllocation(uint256 m) external pure returns (uint256) {
        uint256[] memory a = new uint256[](m);
        return a.length + m;
    }
}
