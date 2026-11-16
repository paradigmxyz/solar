//@ codegen-matrix: standard lowered debug
//@[lowered] compile-flags: -Ogas -Zdump=mir
//@[lowered] filecheck: --check-prefix=LOWERED
//@[debug] compile-flags: -Osize --revert-strings debug -Zdump=mir
//@[debug] filecheck: --check-prefix=DEBUG
//@ run-call: pair ((1,0x11),(2,0x2233)) => 1, 2, 2
//@ run-call: pairs [((1,0x),(2,0x33)),((4,0x),(5,0x))] => 6
//@[gas,size,mir,lowered] run-call-fail: 0x7a5de1e90000000000000000000000000000000000000000000000010000000000000000 => 0x
//@[debug] run-call-fail: 0x7a5de1e90000000000000000000000000000000000000000000000010000000000000000 => Error("ABI decoding: invalid tuple offset")
//@[gas,size,mir,lowered] run-call-fail: 0x7a5de1e9000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000040 => 0x
//@[debug] run-call-fail: 0x7a5de1e9000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000040 => Error("ABI decoding: invalid struct offset")

// A dynamic struct repeated inside calldata parameters decodes through one
// shared helper, which enclosing decoders call with their own tuple base.
// Offsets past the end of calldata revert inside the helper. Debug revert
// strings report nested offsets inline, so only repeated parameter types share
// a helper, and no type here repeats as a parameter.
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

    // DEBUG-NOT: {{^}}fn @decode_calldata_type
}
