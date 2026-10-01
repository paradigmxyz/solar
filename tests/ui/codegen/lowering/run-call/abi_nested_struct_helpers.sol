//@ codegen-matrix: standard lowered
//@[lowered] compile-flags: -Ogas -Zdump=mir
//@[lowered] filecheck: --check-prefix=LOWERED
//@ run-call: pair ((1,0x11),(2,0x2233)) => 1, 2, 2
//@ run-call: pairs [((1,0x),(2,0x33)),((4,0x),(5,0x))] => 6

// A dynamic struct repeated inside calldata parameters decodes through one
// shared helper, which enclosing decoders call with their own tuple base.
contract NestedStructHelpers {
    struct Item {
        uint256 id;
        bytes data;
    }

    struct Pair {
        Item left;
        Item right;
    }

    // LOWERED-LABEL: fn @pair()
    // LOWERED: icall @[[PAIR:decode_calldata_type[.0-9]*]], 4, 4
    function pair(Pair memory p) external pure returns (uint256, uint256, uint256) {
        return (p.left.id, p.right.id, p.right.data.length);
    }

    // LOWERED-LABEL: fn @pairs()
    // LOWERED: icall @[[PAIR]], {{v[0-9]+}}, {{v[0-9]+}}
    function pairs(Pair[] memory ps) external pure returns (uint256 sum) {
        for (uint256 i; i < ps.length; ++i) {
            sum += ps[i].left.id + ps[i].right.data.length;
        }
    }

    // LOWERED: {{^}}fn @[[PAIR]](
    // LOWERED-NOT: {{^}}fn @
    // LOWERED: icall @[[ITEM:decode_calldata_type[.0-9]*]],
    // LOWERED-NOT: {{^}}fn @
    // LOWERED: icall @[[ITEM]],
}
