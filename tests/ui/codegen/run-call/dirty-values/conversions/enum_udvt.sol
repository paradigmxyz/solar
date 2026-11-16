//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_to_enum => 0
//@ run-call: Cu8_0::t_retdirect_to_enum => 0
//@ run-call: Cu8_0::t_ret_to_enum => 0
//@ run-call: Cu8_0::t_param_to_enum => 0
//@ run-call: Cu8_0::t_copy_to_enum => 0
//@ run-call: Cu8_0::t_ternary_to_enum => 0
//@ run-call: Cu8_0::t_ifjoin_to_enum => 0
//@ run-call: Cu8_0::t_loopjoin_to_enum => 0
//@ run-call: Cu8_0::t_tuple_to_enum => 0
//@ run-call: Cu8_0::t_modifier_to_enum => 0
//@ run-call: Cu8_0::t_ptr_to_enum => 0
//@ run-call: Cu8_0::t_mem_to_enum => 0
//@ run-call: Cu64_0::t_local_to_enum => 0
//@ run-call: Cu64_0::t_retdirect_to_enum => 0
//@ run-call: Cu64_0::t_param_to_enum => 0
//@ run-call: Cu64_0::t_mem_to_enum => 0
//@ run-call-fail: Cen_0::t_local_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_roundtrip => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_unwrap_wide => 27
//@ run-call: Cud_0::t_retdirect_unwrap_wide => 27
//@ run-call: Cud_0::t_param_unwrap_wide => 27
//@ run-call: Cud_0::t_mem_unwrap_wide => 27
//@ run-call: Cud_0::t_local_unwrap_u256 => 27
//@ run-call: Cud_0::t_retdirect_unwrap_u256 => 27
//@ run-call: Cud_0::t_param_unwrap_u256 => 27
//@ run-call: Cud_0::t_mem_unwrap_u256 => 27
//@ run-call: Cud_0::t_local_rewrap => 27
//@ run-call: Cud_0::t_retdirect_rewrap => 27
//@ run-call: Cud_0::t_param_rewrap => 27
//@ run-call: Cud_0::t_mem_rewrap => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: conversions of dirty values.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

contract Cu8_0 {
    function _s_retdirect_to_enum() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_enum() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_enum(uint8 pv) internal view  returns (uint256) { return uint256(uint8(E(uint8(pv % 3)))); }
    uint256 _res_modifier_to_enum;
    modifier _m_modifier_to_enum(uint8 pv) { _res_modifier_to_enum = uint256(uint8(E(uint8(pv % 3)))); _; }
    function _e_modifier_to_enum(uint8 rd) internal _m_modifier_to_enum(rd)  returns (uint256) { return _res_modifier_to_enum; }
    function _w_ptr_to_enum() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(E(uint8(rx % 3)))); }
    function t_retdirect_to_enum() external view  returns (uint256) { return uint256(uint8(E(uint8(_s_retdirect_to_enum() % 3)))); }
    function t_ret_to_enum() external view  returns (uint256) { uint8 rx = _s_ret_to_enum(); return uint256(uint8(E(uint8(rx % 3)))); }
    function t_param_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_enum(rx); }
    function t_copy_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_ternary_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_ifjoin_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(E(uint8(ry % 3)))); }
    function t_loopjoin_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(E(uint8(ry % 3)))); }
    function t_tuple_to_enum() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(E(uint8(ry % 3)))); }
    function t_modifier_to_enum() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_enum(rx); }
    function t_ptr_to_enum() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_enum; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(E(uint8(ry % 3)))); }
    function t_mem_to_enum() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(E(uint8(ry % 3)))); }
}
contract Cu64_0 {
    function _s_retdirect_to_enum() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_enum(uint64 pv) internal view  returns (uint256) { return uint256(uint8(E(uint8(pv % 3)))); }
    function t_local_to_enum() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(E(uint8(rx % 3)))); }
    function t_retdirect_to_enum() external view  returns (uint256) { return uint256(uint8(E(uint8(_s_retdirect_to_enum() % 3)))); }
    function t_param_to_enum() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_enum(rx); }
    function t_mem_to_enum() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(E(uint8(ry % 3)))); }
}
contract Cen_0 {
    function _s_retdirect_roundtrip() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_roundtrip() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_roundtrip(E pv) internal view  returns (uint256) { return uint256(uint8(E(uint8(uint8(pv))))); }
    uint256 _res_modifier_roundtrip;
    modifier _m_modifier_roundtrip(E pv) { _res_modifier_roundtrip = uint256(uint8(E(uint8(uint8(pv))))); _; }
    function _e_modifier_roundtrip(E rd) internal _m_modifier_roundtrip(rd)  returns (uint256) { return _res_modifier_roundtrip; }
    function _w_ptr_roundtrip() internal pure returns (uint256) { return 0x101; }
    function t_local_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint8(E(uint8(uint8(rx))))); }
    function t_retdirect_roundtrip() external view  returns (uint256) { return uint256(uint8(E(uint8(uint8(_s_retdirect_roundtrip()))))); }
    function t_ret_roundtrip() external view  returns (uint256) { E rx = _s_ret_roundtrip(); return uint256(uint8(E(uint8(uint8(rx))))); }
    function t_param_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_roundtrip(rx); }
    function t_copy_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_ternary_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_ifjoin_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_loopjoin_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_tuple_roundtrip() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_modifier_roundtrip() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_roundtrip(rx); }
    function t_ptr_roundtrip() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_roundtrip; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint8(E(uint8(uint8(ry))))); }
    function t_mem_roundtrip() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint8(E(uint8(uint8(ry))))); }
}
contract Cud_0 {
    function _s_retdirect_unwrap_wide() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_wide(U pv) internal view  returns (uint256) { return uint256(uint16(U.unwrap(pv))); }
    function _s_retdirect_unwrap_u256() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_u256(U pv) internal view  returns (uint256) { return uint256(U.unwrap(pv)); }
    function _s_retdirect_rewrap() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_rewrap(U pv) internal view  returns (uint256) { return uint256(U.unwrap(U.wrap(U.unwrap(pv)))); }
    function t_local_unwrap_wide() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(uint16(U.unwrap(rx))); }
    function t_retdirect_unwrap_wide() external view  returns (uint256) { return uint256(uint16(U.unwrap(_s_retdirect_unwrap_wide()))); }
    function t_param_unwrap_wide() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_wide(rx); }
    function t_mem_unwrap_wide() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(uint16(U.unwrap(ry))); }
    function t_local_unwrap_u256() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(rx)); }
    function t_retdirect_unwrap_u256() external view  returns (uint256) { return uint256(U.unwrap(_s_retdirect_unwrap_u256())); }
    function t_param_unwrap_u256() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_u256(rx); }
    function t_mem_unwrap_u256() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(ry)); }
    function t_local_rewrap() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(U.wrap(U.unwrap(rx)))); }
    function t_retdirect_rewrap() external view  returns (uint256) { return uint256(U.unwrap(U.wrap(U.unwrap(_s_retdirect_rewrap())))); }
    function t_param_rewrap() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_rewrap(rx); }
    function t_mem_rewrap() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(U.wrap(U.unwrap(ry)))); }
}
