//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_retdirect_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_param_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_mem_to_uint_via_ternary => 1
//@ run-call: Cbool_0::t_local_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_retdirect_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_param_to_uint_via_bytes => 1
//@ run-call: Cbool_0::t_mem_to_uint_via_bytes => 1
//@ run-call-fail: Cen_0::t_local_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint16 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

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

contract Cbool_0 {
    function _s_retdirect_to_uint_via_ternary() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_to_uint_via_ternary(bool pv) internal view  returns (uint256) { return pv ? 1 : 0; }
    function _s_retdirect_to_uint_via_bytes() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_to_uint_via_bytes(bool pv) internal view  returns (uint256) { return uint256(uint8(pv ? 1 : 0)); }
    function t_local_to_uint_via_ternary() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return rx ? 1 : 0; }
    function t_retdirect_to_uint_via_ternary() external view  returns (uint256) { return _s_retdirect_to_uint_via_ternary() ? 1 : 0; }
    function t_param_to_uint_via_ternary() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_to_uint_via_ternary(rx); }
    function t_mem_to_uint_via_ternary() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return ry ? 1 : 0; }
    function t_local_to_uint_via_bytes() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(uint8(rx ? 1 : 0)); }
    function t_retdirect_to_uint_via_bytes() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_via_bytes() ? 1 : 0)); }
    function t_param_to_uint_via_bytes() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_to_uint_via_bytes(rx); }
    function t_mem_to_uint_via_bytes() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(uint8(ry ? 1 : 0)); }
}
contract Cen_0 {
    function _s_retdirect_to_uint8() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint8() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint8(E pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    uint256 _res_modifier_to_uint8;
    modifier _m_modifier_to_uint8(E pv) { _res_modifier_to_uint8 = uint256(uint8(pv)); _; }
    function _e_modifier_to_uint8(E rd) internal _m_modifier_to_uint8(rd)  returns (uint256) { return _res_modifier_to_uint8; }
    function _w_ptr_to_uint8() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_to_uint256() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint256() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint256(E pv) internal view  returns (uint256) { return uint256(uint256(uint8(pv))); }
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(E pv) { _res_modifier_to_uint256 = uint256(uint256(uint8(pv))); _; }
    function _e_modifier_to_uint256(E rd) internal _m_modifier_to_uint256(rd)  returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_to_uint16() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint16() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint16(E pv) internal view  returns (uint256) { return uint256(uint16(uint8(pv))); }
    uint256 _res_modifier_to_uint16;
    modifier _m_modifier_to_uint16(E pv) { _res_modifier_to_uint16 = uint256(uint16(uint8(pv))); _; }
    function _e_modifier_to_uint16(E rd) internal _m_modifier_to_uint16(rd)  returns (uint256) { return _res_modifier_to_uint16; }
    function _w_ptr_to_uint16() internal pure returns (uint256) { return 0x101; }
    function t_local_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint8(rx)); }
    function t_retdirect_to_uint8() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint8())); }
    function t_ret_to_uint8() external view  returns (uint256) { E rx = _s_ret_to_uint8(); return uint256(uint8(rx)); }
    function t_param_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint8(rx); }
    function t_copy_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint8(ry)); }
    function t_ternary_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint8(ry)); }
    function t_ifjoin_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry)); }
    function t_loopjoin_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry)); }
    function t_tuple_to_uint8() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry)); }
    function t_modifier_to_uint8() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint8(rx); }
    function t_ptr_to_uint8() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint8; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint8(ry)); }
    function t_mem_to_uint8() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint256(uint8(rx))); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint256(uint8(_s_retdirect_to_uint256()))); }
    function t_ret_to_uint256() external view  returns (uint256) { E rx = _s_ret_to_uint256(); return uint256(uint256(uint8(rx))); }
    function t_param_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint256(rx); }
    function t_copy_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint256(uint8(ry))); }
    function t_ternary_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint256(uint8(ry))); }
    function t_ifjoin_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(uint8(ry))); }
    function t_loopjoin_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(uint8(ry))); }
    function t_tuple_to_uint256() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(uint8(ry))); }
    function t_modifier_to_uint256() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint256(uint8(ry))); }
    function t_mem_to_uint256() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint256(uint8(ry))); }
    function t_local_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint16(uint8(rx))); }
    function t_retdirect_to_uint16() external view  returns (uint256) { return uint256(uint16(uint8(_s_retdirect_to_uint16()))); }
    function t_ret_to_uint16() external view  returns (uint256) { E rx = _s_ret_to_uint16(); return uint256(uint16(uint8(rx))); }
    function t_param_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint16(rx); }
    function t_copy_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint16(uint8(ry))); }
    function t_ternary_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint16(uint8(ry))); }
    function t_ifjoin_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(uint8(ry))); }
    function t_loopjoin_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(uint8(ry))); }
    function t_tuple_to_uint16() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(uint8(ry))); }
    function t_modifier_to_uint16() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint16(rx); }
    function t_ptr_to_uint16() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint16; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint16(uint8(ry))); }
    function t_mem_to_uint16() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint16(uint8(ry))); }
}
