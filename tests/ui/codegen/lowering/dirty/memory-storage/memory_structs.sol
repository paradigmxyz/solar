//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_mstruct_field => 27
//@ run-call: Cu8_0::t_retdirect_mstruct_field => 27
//@ run-call: Cu8_0::t_param_mstruct_field => 27
//@ run-call: Cu8_0::t_mem_mstruct_field => 27
//@ run-call: Cu8_0::t_local_mstruct_literal => 27
//@ run-call: Cu8_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cu8_0::t_param_mstruct_literal => 27
//@ run-call: Cu8_0::t_mem_mstruct_literal => 27
//@ run-call: Cu64_0::t_local_mstruct_field => 27
//@ run-call: Cu64_0::t_retdirect_mstruct_field => 27
//@ run-call: Cu64_0::t_param_mstruct_field => 27
//@ run-call: Cu64_0::t_mem_mstruct_field => 27
//@ run-call: Cu64_0::t_local_mstruct_literal => 27
//@ run-call: Cu64_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cu64_0::t_param_mstruct_literal => 27
//@ run-call: Cu64_0::t_mem_mstruct_literal => 27
//@ run-call: Ci8_0::t_local_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mstruct_field => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mstruct_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Caddr_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mstruct_field => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mstruct_literal => 103929005307130220006098923584552504982110632080
//@ run-call: Cbool_0::t_local_mstruct_field => 1
//@ run-call: Cbool_0::t_retdirect_mstruct_field => 1
//@ run-call: Cbool_0::t_param_mstruct_field => 1
//@ run-call: Cbool_0::t_mem_mstruct_field => 1
//@ run-call: Cbool_0::t_local_mstruct_literal => 1
//@ run-call: Cbool_0::t_retdirect_mstruct_literal => 1
//@ run-call: Cbool_0::t_param_mstruct_literal => 1
//@ run-call: Cbool_0::t_mem_mstruct_literal => 1
//@ run-call: Cb1_0::t_local_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mstruct_field => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mstruct_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb4_0::t_local_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mstruct_field => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mstruct_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb20_0::t_local_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mstruct_field => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mstruct_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call-fail: Cen_0::t_local_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mstruct_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mstruct_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_mstruct_field => 27
//@ run-call: Cud_0::t_retdirect_mstruct_field => 27
//@ run-call: Cud_0::t_param_mstruct_field => 27
//@ run-call: Cud_0::t_mem_mstruct_field => 27
//@ run-call: Cud_0::t_local_mstruct_literal => 27
//@ run-call: Cud_0::t_retdirect_mstruct_literal => 27
//@ run-call: Cud_0::t_param_mstruct_literal => 27
//@ run-call: Cud_0::t_mem_mstruct_literal => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: memory and storage stores, keys, indexes, slices and allocations.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

struct St_u8 {
    uint256 a;
    uint8 f;
}

struct St_u64 {
    uint256 a;
    uint64 f;
}

struct St_i8 {
    uint256 a;
    int8 f;
}

struct St_i24 {
    uint256 a;
    int24 f;
}

struct St_addr {
    uint256 a;
    address f;
}

struct St_addrp {
    uint256 a;
    address payable f;
}

struct St_ctr {
    uint256 a;
    IC f;
}

struct St_bool {
    uint256 a;
    bool f;
}

struct St_b1 {
    uint256 a;
    bytes1 f;
}

struct St_b4 {
    uint256 a;
    bytes4 f;
}

struct St_b20 {
    uint256 a;
    bytes20 f;
}

struct St_en {
    uint256 a;
    E f;
}

struct St_ud {
    uint256 a;
    U f;
}

contract Cu8_0 {
    function _s_retdirect_mstruct_field() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_field(uint8 pv) internal view  returns (uint256) { St_u8 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_literal(uint8 pv) internal view  returns (uint256) { St_u8 memory s = St_u8(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_u8 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s = St_u8(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_u8 memory s = St_u8(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s = St_u8(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cu64_0 {
    function _s_retdirect_mstruct_field() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mstruct_field(uint64 pv) internal view  returns (uint256) { St_u64 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mstruct_literal(uint64 pv) internal view  returns (uint256) { St_u64 memory s = St_u64(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_u64 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s = St_u64(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_u64 memory s = St_u64(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s = St_u64(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Ci8_0 {
    function _s_retdirect_mstruct_field() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mstruct_field(int8 pv) internal view  returns (uint256) { St_i8 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mstruct_literal(int8 pv) internal view  returns (uint256) { St_i8 memory s = St_i8(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_i8 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s = St_i8(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_i8 memory s = St_i8(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s = St_i8(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Ci24_0 {
    function _s_retdirect_mstruct_field() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mstruct_field(int24 pv) internal view  returns (uint256) { St_i24 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mstruct_literal(int24 pv) internal view  returns (uint256) { St_i24 memory s = St_i24(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_i24 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s = St_i24(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_i24 memory s = St_i24(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s = St_i24(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Caddr_0 {
    function _s_retdirect_mstruct_field() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(address pv) internal view  returns (uint256) { St_addr memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(address pv) internal view  returns (uint256) { St_addr memory s = St_addr(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_addr memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s = St_addr(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_addr memory s = St_addr(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s = St_addr(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Caddrp_0 {
    function _s_retdirect_mstruct_field() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(address payable pv) internal view  returns (uint256) { St_addrp memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(address payable pv) internal view  returns (uint256) { St_addrp memory s = St_addrp(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_addrp memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s = St_addrp(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_addrp memory s = St_addrp(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s = St_addrp(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cctr_0 {
    function _s_retdirect_mstruct_field() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_field(IC pv) internal view  returns (uint256) { St_ctr memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mstruct_literal(IC pv) internal view  returns (uint256) { St_ctr memory s = St_ctr(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_ctr memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s = St_ctr(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_ctr memory s = St_ctr(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s = St_ctr(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cbool_0 {
    function _s_retdirect_mstruct_field() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mstruct_field(bool pv) internal view  returns (uint256) { St_bool memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mstruct_literal(bool pv) internal view  returns (uint256) { St_bool memory s = St_bool(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_bool memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s = St_bool(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_bool memory s = St_bool(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s = St_bool(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cb1_0 {
    function _s_retdirect_mstruct_field() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mstruct_field(bytes1 pv) internal view  returns (uint256) { St_b1 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mstruct_literal(bytes1 pv) internal view  returns (uint256) { St_b1 memory s = St_b1(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_b1 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s = St_b1(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_b1 memory s = St_b1(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s = St_b1(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cb4_0 {
    function _s_retdirect_mstruct_field() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mstruct_field(bytes4 pv) internal view  returns (uint256) { St_b4 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mstruct_literal(bytes4 pv) internal view  returns (uint256) { St_b4 memory s = St_b4(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_b4 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s = St_b4(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_b4 memory s = St_b4(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s = St_b4(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cb20_0 {
    function _s_retdirect_mstruct_field() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mstruct_field(bytes20 pv) internal view  returns (uint256) { St_b20 memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mstruct_literal(bytes20 pv) internal view  returns (uint256) { St_b20 memory s = St_b20(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_b20 memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s = St_b20(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_b20 memory s = St_b20(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s = St_b20(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cen_0 {
    function _s_retdirect_mstruct_field() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mstruct_field(E pv) internal view  returns (uint256) { St_en memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mstruct_literal(E pv) internal view  returns (uint256) { St_en memory s = St_en(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_en memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s = St_en(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_en memory s = St_en(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s = St_en(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
contract Cud_0 {
    function _s_retdirect_mstruct_field() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_field(U pv) internal view  returns (uint256) { St_ud memory s; s.f = pv; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function _s_retdirect_mstruct_literal() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mstruct_literal(U pv) internal view  returns (uint256) { St_ud memory s = St_ud(1, pv); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_field() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s; s.f = rx; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_field() external view  returns (uint256) { St_ud memory s; s.f = _s_retdirect_mstruct_field(); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_field() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_field(rx); }
    function t_mem_mstruct_field() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s; s.f = ry; uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_local_mstruct_literal() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s = St_ud(1, rx); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_retdirect_mstruct_literal() external view  returns (uint256) { St_ud memory s = St_ud(1, _s_retdirect_mstruct_literal()); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
    function t_param_mstruct_literal() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mstruct_literal(rx); }
    function t_mem_mstruct_literal() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s = St_ud(1, ry); uint256 r; assembly { r := mload(add(s, 32)) } return r; }
}
