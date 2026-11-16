//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_cmp_all => 682
//@ run-call: Cu8_0::t_retdirect_cmp_all => 682
//@ run-call: Cu8_0::t_ret_cmp_all => 682
//@ run-call: Cu8_0::t_param_cmp_all => 682
//@ run-call: Cu8_0::t_copy_cmp_all => 682
//@ run-call: Cu8_0::t_ternary_cmp_all => 682
//@ run-call: Cu8_0::t_ifjoin_cmp_all => 682
//@ run-call: Cu8_0::t_loopjoin_cmp_all => 682
//@ run-call: Cu8_0::t_tuple_cmp_all => 682
//@ run-call: Cu8_0::t_modifier_cmp_all => 682
//@ run-call: Cu8_0::t_ptr_cmp_all => 682
//@ run-call: Cu8_0::t_mem_cmp_all => 682
//@ run-call: Cu8_0::t_local_cmp_eq => 0
//@ run-call: Cu8_0::t_retdirect_cmp_eq => 0
//@ run-call: Cu8_0::t_ret_cmp_eq => 0
//@ run-call: Cu8_0::t_param_cmp_eq => 0
//@ run-call: Cu8_0::t_copy_cmp_eq => 0
//@ run-call: Cu8_0::t_ternary_cmp_eq => 0
//@ run-call: Cu8_0::t_ifjoin_cmp_eq => 0
//@ run-call: Cu8_0::t_loopjoin_cmp_eq => 0
//@ run-call: Cu8_0::t_tuple_cmp_eq => 0
//@ run-call: Cu8_0::t_modifier_cmp_eq => 0
//@ run-call: Cu8_0::t_ptr_cmp_eq => 0
//@ run-call: Cu8_0::t_mem_cmp_eq => 0
//@ run-call: Cu8_0::t_local_cmp_lt => 0
//@ run-call: Cu8_0::t_retdirect_cmp_lt => 0
//@ run-call: Cu8_0::t_ret_cmp_lt => 0
//@ run-call: Cu8_0::t_param_cmp_lt => 0
//@ run-call: Cu8_0::t_copy_cmp_lt => 0
//@ run-call: Cu8_0::t_ternary_cmp_lt => 0
//@ run-call: Cu8_0::t_ifjoin_cmp_lt => 0
//@ run-call: Cu8_0::t_loopjoin_cmp_lt => 0
//@ run-call: Cu8_0::t_tuple_cmp_lt => 0
//@ run-call: Cu8_0::t_modifier_cmp_lt => 0
//@ run-call: Cu8_0::t_ptr_cmp_lt => 0
//@ run-call: Cu8_0::t_mem_cmp_lt => 0
//@ run-call: Cu8_0::t_local_cmp_gt => 0
//@ run-call: Cu8_0::t_retdirect_cmp_gt => 0
//@ run-call: Cu8_0::t_ret_cmp_gt => 0
//@ run-call: Cu8_0::t_param_cmp_gt => 0
//@ run-call: Cu8_1::t_copy_cmp_gt => 0
//@ run-call: Cu8_1::t_ternary_cmp_gt => 0
//@ run-call: Cu8_1::t_ifjoin_cmp_gt => 0
//@ run-call: Cu8_1::t_loopjoin_cmp_gt => 0
//@ run-call: Cu8_1::t_tuple_cmp_gt => 0
//@ run-call: Cu8_1::t_modifier_cmp_gt => 0
//@ run-call: Cu8_1::t_ptr_cmp_gt => 0
//@ run-call: Cu8_1::t_mem_cmp_gt => 0
//@ run-call: Cu64_0::t_local_cmp_all => 682
//@ run-call: Cu64_0::t_retdirect_cmp_all => 682
//@ run-call: Cu64_0::t_param_cmp_all => 682
//@ run-call: Cu64_0::t_mem_cmp_all => 682
//@ run-call: Cu64_0::t_local_cmp_eq => 0
//@ run-call: Cu64_0::t_retdirect_cmp_eq => 0
//@ run-call: Cu64_0::t_param_cmp_eq => 0
//@ run-call: Cu64_0::t_mem_cmp_eq => 0
//@ run-call: Cu64_0::t_local_cmp_lt => 0
//@ run-call: Cu64_0::t_retdirect_cmp_lt => 0
//@ run-call: Cu64_0::t_param_cmp_lt => 0
//@ run-call: Cu64_0::t_mem_cmp_lt => 0
//@ run-call: Cu64_0::t_local_cmp_gt => 0
//@ run-call: Cu64_0::t_retdirect_cmp_gt => 0
//@ run-call: Cu64_0::t_param_cmp_gt => 0
//@ run-call: Cu64_0::t_mem_cmp_gt => 0
//@ run-call: Ci8_0::t_local_cmp_all => 790
//@ run-call: Ci8_0::t_retdirect_cmp_all => 790
//@ run-call: Ci8_0::t_param_cmp_all => 790
//@ run-call: Ci8_0::t_mem_cmp_all => 790
//@ run-call: Ci8_0::t_local_cmp_eq => 0
//@ run-call: Ci8_0::t_retdirect_cmp_eq => 0
//@ run-call: Ci8_0::t_param_cmp_eq => 0
//@ run-call: Ci8_0::t_mem_cmp_eq => 0
//@ run-call: Ci8_0::t_local_cmp_lt => 1
//@ run-call: Ci8_0::t_retdirect_cmp_lt => 1
//@ run-call: Ci8_0::t_param_cmp_lt => 1
//@ run-call: Ci8_0::t_mem_cmp_lt => 1
//@ run-call: Ci8_0::t_local_cmp_gt => 1
//@ run-call: Ci8_0::t_retdirect_cmp_gt => 1
//@ run-call: Ci8_0::t_param_cmp_gt => 1
//@ run-call: Ci8_0::t_mem_cmp_gt => 1
//@ run-call: Ci24_0::t_local_cmp_all => 790
//@ run-call: Ci24_0::t_retdirect_cmp_all => 790
//@ run-call: Ci24_0::t_param_cmp_all => 790
//@ run-call: Ci24_0::t_mem_cmp_all => 790
//@ run-call: Ci24_0::t_local_cmp_eq => 0
//@ run-call: Ci24_0::t_retdirect_cmp_eq => 0
//@ run-call: Ci24_0::t_param_cmp_eq => 0
//@ run-call: Ci24_0::t_mem_cmp_eq => 0
//@ run-call: Ci24_0::t_local_cmp_lt => 1
//@ run-call: Ci24_0::t_retdirect_cmp_lt => 1
//@ run-call: Ci24_0::t_param_cmp_lt => 1
//@ run-call: Ci24_0::t_mem_cmp_lt => 1
//@ run-call: Ci24_0::t_local_cmp_gt => 1
//@ run-call: Ci24_0::t_retdirect_cmp_gt => 1
//@ run-call: Ci24_0::t_param_cmp_gt => 1
//@ run-call: Ci24_0::t_mem_cmp_gt => 1

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: operators on dirty operands.

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
    function _s_retdirect_cmp_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_cmp_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp_all(uint8 pv) internal view  returns (uint256) { return ((pv == uint8(3)) ? uint256(1) : 0) | ((pv != uint8(3)) ? uint256(2) : 0) | ((pv < uint8(3)) ? uint256(4) : 0) | ((pv > uint8(3)) ? uint256(8) : 0) | ((pv <= uint8(3)) ? uint256(16) : 0) | ((pv >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == pv) ? uint256(64) : 0) | ((uint8(3) < pv) ? uint256(128) : 0) | ((uint8(3) > pv) ? uint256(256) : 0) | ((pv == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != pv) ? uint256(1024) : 0); }
    uint256 _res_modifier_cmp_all;
    modifier _m_modifier_cmp_all(uint8 pv) { _res_modifier_cmp_all = ((pv == uint8(3)) ? uint256(1) : 0) | ((pv != uint8(3)) ? uint256(2) : 0) | ((pv < uint8(3)) ? uint256(4) : 0) | ((pv > uint8(3)) ? uint256(8) : 0) | ((pv <= uint8(3)) ? uint256(16) : 0) | ((pv >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == pv) ? uint256(64) : 0) | ((uint8(3) < pv) ? uint256(128) : 0) | ((uint8(3) > pv) ? uint256(256) : 0) | ((pv == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != pv) ? uint256(1024) : 0); _; }
    function _e_modifier_cmp_all(uint8 rd) internal _m_modifier_cmp_all(rd)  returns (uint256) { return _res_modifier_cmp_all; }
    function _w_ptr_cmp_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_cmp_eq() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_cmp_eq() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp_eq(uint8 pv) internal view  returns (uint256) { return (pv == uint8(3)) ? 1 : 0; }
    uint256 _res_modifier_cmp_eq;
    modifier _m_modifier_cmp_eq(uint8 pv) { _res_modifier_cmp_eq = (pv == uint8(3)) ? 1 : 0; _; }
    function _e_modifier_cmp_eq(uint8 rd) internal _m_modifier_cmp_eq(rd)  returns (uint256) { return _res_modifier_cmp_eq; }
    function _w_ptr_cmp_eq() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_cmp_lt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_cmp_lt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp_lt(uint8 pv) internal view  returns (uint256) { return (pv < uint8(3)) ? 1 : 0; }
    uint256 _res_modifier_cmp_lt;
    modifier _m_modifier_cmp_lt(uint8 pv) { _res_modifier_cmp_lt = (pv < uint8(3)) ? 1 : 0; _; }
    function _e_modifier_cmp_lt(uint8 rd) internal _m_modifier_cmp_lt(rd)  returns (uint256) { return _res_modifier_cmp_lt; }
    function _w_ptr_cmp_lt() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_cmp_gt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_cmp_gt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp_gt(uint8 pv) internal view  returns (uint256) { return (uint8(3) > pv) ? 1 : 0; }
    function t_local_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return ((rx == uint8(3)) ? uint256(1) : 0) | ((rx != uint8(3)) ? uint256(2) : 0) | ((rx < uint8(3)) ? uint256(4) : 0) | ((rx > uint8(3)) ? uint256(8) : 0) | ((rx <= uint8(3)) ? uint256(16) : 0) | ((rx >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == rx) ? uint256(64) : 0) | ((uint8(3) < rx) ? uint256(128) : 0) | ((uint8(3) > rx) ? uint256(256) : 0) | ((rx == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != rx) ? uint256(1024) : 0); }
    function t_retdirect_cmp_all() external view  returns (uint256) { return ((_s_retdirect_cmp_all() == uint8(3)) ? uint256(1) : 0) | ((_s_retdirect_cmp_all() != uint8(3)) ? uint256(2) : 0) | ((_s_retdirect_cmp_all() < uint8(3)) ? uint256(4) : 0) | ((_s_retdirect_cmp_all() > uint8(3)) ? uint256(8) : 0) | ((_s_retdirect_cmp_all() <= uint8(3)) ? uint256(16) : 0) | ((_s_retdirect_cmp_all() >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == _s_retdirect_cmp_all()) ? uint256(64) : 0) | ((uint8(3) < _s_retdirect_cmp_all()) ? uint256(128) : 0) | ((uint8(3) > _s_retdirect_cmp_all()) ? uint256(256) : 0) | ((_s_retdirect_cmp_all() == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != _s_retdirect_cmp_all()) ? uint256(1024) : 0); }
    function t_ret_cmp_all() external view  returns (uint256) { uint8 rx = _s_ret_cmp_all(); return ((rx == uint8(3)) ? uint256(1) : 0) | ((rx != uint8(3)) ? uint256(2) : 0) | ((rx < uint8(3)) ? uint256(4) : 0) | ((rx > uint8(3)) ? uint256(8) : 0) | ((rx <= uint8(3)) ? uint256(16) : 0) | ((rx >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == rx) ? uint256(64) : 0) | ((uint8(3) < rx) ? uint256(128) : 0) | ((uint8(3) > rx) ? uint256(256) : 0) | ((rx == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != rx) ? uint256(1024) : 0); }
    function t_param_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_cmp_all(rx); }
    function t_copy_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_ternary_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_ifjoin_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_loopjoin_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_tuple_cmp_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_modifier_cmp_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_cmp_all(rx); }
    function t_ptr_cmp_all() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_mem_cmp_all() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return ((ry == uint8(3)) ? uint256(1) : 0) | ((ry != uint8(3)) ? uint256(2) : 0) | ((ry < uint8(3)) ? uint256(4) : 0) | ((ry > uint8(3)) ? uint256(8) : 0) | ((ry <= uint8(3)) ? uint256(16) : 0) | ((ry >= uint8(3)) ? uint256(32) : 0) | ((uint8(3) == ry) ? uint256(64) : 0) | ((uint8(3) < ry) ? uint256(128) : 0) | ((uint8(3) > ry) ? uint256(256) : 0) | ((ry == uint8(27)) ? uint256(512) : 0) | ((uint8(27) != ry) ? uint256(1024) : 0); }
    function t_local_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return (rx == uint8(3)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == uint8(3)) ? 1 : 0; }
    function t_ret_cmp_eq() external view  returns (uint256) { uint8 rx = _s_ret_cmp_eq(); return (rx == uint8(3)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_cmp_eq(rx); }
    function t_copy_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return (ry == uint8(3)) ? 1 : 0; }
    function t_ternary_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return (ry == uint8(3)) ? 1 : 0; }
    function t_ifjoin_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == uint8(3)) ? 1 : 0; }
    function t_loopjoin_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == uint8(3)) ? 1 : 0; }
    function t_tuple_cmp_eq() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == uint8(3)) ? 1 : 0; }
    function t_modifier_cmp_eq() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_cmp_eq(rx); }
    function t_ptr_cmp_eq() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_eq; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return (ry == uint8(3)) ? 1 : 0; }
    function t_mem_cmp_eq() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return (ry == uint8(3)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return (rx < uint8(3)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < uint8(3)) ? 1 : 0; }
    function t_ret_cmp_lt() external view  returns (uint256) { uint8 rx = _s_ret_cmp_lt(); return (rx < uint8(3)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_cmp_lt(rx); }
    function t_copy_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return (ry < uint8(3)) ? 1 : 0; }
    function t_ternary_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return (ry < uint8(3)) ? 1 : 0; }
    function t_ifjoin_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry < uint8(3)) ? 1 : 0; }
    function t_loopjoin_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry < uint8(3)) ? 1 : 0; }
    function t_tuple_cmp_lt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry < uint8(3)) ? 1 : 0; }
    function t_modifier_cmp_lt() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_cmp_lt(rx); }
    function t_ptr_cmp_lt() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_lt; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return (ry < uint8(3)) ? 1 : 0; }
    function t_mem_cmp_lt() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return (ry < uint8(3)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return (uint8(3) > rx) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (uint8(3) > _s_retdirect_cmp_gt()) ? 1 : 0; }
    function t_ret_cmp_gt() external view  returns (uint256) { uint8 rx = _s_ret_cmp_gt(); return (uint8(3) > rx) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_cmp_gt(rx); }
}
contract Cu8_1 {
    uint256 _res_modifier_cmp_gt;
    modifier _m_modifier_cmp_gt(uint8 pv) { _res_modifier_cmp_gt = (uint8(3) > pv) ? 1 : 0; _; }
    function _e_modifier_cmp_gt(uint8 rd) internal _m_modifier_cmp_gt(rd)  returns (uint256) { return _res_modifier_cmp_gt; }
    function _w_ptr_cmp_gt() internal pure returns (uint256) { return 0xabcd1b; }
    function t_copy_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return (uint8(3) > ry) ? 1 : 0; }
    function t_ternary_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return (uint8(3) > ry) ? 1 : 0; }
    function t_ifjoin_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return (uint8(3) > ry) ? 1 : 0; }
    function t_loopjoin_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (uint8(3) > ry) ? 1 : 0; }
    function t_tuple_cmp_gt() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (uint8(3) > ry) ? 1 : 0; }
    function t_modifier_cmp_gt() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_cmp_gt(rx); }
    function t_ptr_cmp_gt() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_gt; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return (uint8(3) > ry) ? 1 : 0; }
    function t_mem_cmp_gt() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return (uint8(3) > ry) ? 1 : 0; }
}
contract Cu64_0 {
    function _s_retdirect_cmp_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_cmp_all(uint64 pv) internal view  returns (uint256) { return ((pv == uint64(3)) ? uint256(1) : 0) | ((pv != uint64(3)) ? uint256(2) : 0) | ((pv < uint64(3)) ? uint256(4) : 0) | ((pv > uint64(3)) ? uint256(8) : 0) | ((pv <= uint64(3)) ? uint256(16) : 0) | ((pv >= uint64(3)) ? uint256(32) : 0) | ((uint64(3) == pv) ? uint256(64) : 0) | ((uint64(3) < pv) ? uint256(128) : 0) | ((uint64(3) > pv) ? uint256(256) : 0) | ((pv == uint64(27)) ? uint256(512) : 0) | ((uint64(27) != pv) ? uint256(1024) : 0); }
    function _s_retdirect_cmp_eq() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_cmp_eq(uint64 pv) internal view  returns (uint256) { return (pv == uint64(3)) ? 1 : 0; }
    function _s_retdirect_cmp_lt() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_cmp_lt(uint64 pv) internal view  returns (uint256) { return (pv < uint64(3)) ? 1 : 0; }
    function _s_retdirect_cmp_gt() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_cmp_gt(uint64 pv) internal view  returns (uint256) { return (uint64(3) > pv) ? 1 : 0; }
    function t_local_cmp_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return ((rx == uint64(3)) ? uint256(1) : 0) | ((rx != uint64(3)) ? uint256(2) : 0) | ((rx < uint64(3)) ? uint256(4) : 0) | ((rx > uint64(3)) ? uint256(8) : 0) | ((rx <= uint64(3)) ? uint256(16) : 0) | ((rx >= uint64(3)) ? uint256(32) : 0) | ((uint64(3) == rx) ? uint256(64) : 0) | ((uint64(3) < rx) ? uint256(128) : 0) | ((uint64(3) > rx) ? uint256(256) : 0) | ((rx == uint64(27)) ? uint256(512) : 0) | ((uint64(27) != rx) ? uint256(1024) : 0); }
    function t_retdirect_cmp_all() external view  returns (uint256) { return ((_s_retdirect_cmp_all() == uint64(3)) ? uint256(1) : 0) | ((_s_retdirect_cmp_all() != uint64(3)) ? uint256(2) : 0) | ((_s_retdirect_cmp_all() < uint64(3)) ? uint256(4) : 0) | ((_s_retdirect_cmp_all() > uint64(3)) ? uint256(8) : 0) | ((_s_retdirect_cmp_all() <= uint64(3)) ? uint256(16) : 0) | ((_s_retdirect_cmp_all() >= uint64(3)) ? uint256(32) : 0) | ((uint64(3) == _s_retdirect_cmp_all()) ? uint256(64) : 0) | ((uint64(3) < _s_retdirect_cmp_all()) ? uint256(128) : 0) | ((uint64(3) > _s_retdirect_cmp_all()) ? uint256(256) : 0) | ((_s_retdirect_cmp_all() == uint64(27)) ? uint256(512) : 0) | ((uint64(27) != _s_retdirect_cmp_all()) ? uint256(1024) : 0); }
    function t_param_cmp_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_cmp_all(rx); }
    function t_mem_cmp_all() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return ((ry == uint64(3)) ? uint256(1) : 0) | ((ry != uint64(3)) ? uint256(2) : 0) | ((ry < uint64(3)) ? uint256(4) : 0) | ((ry > uint64(3)) ? uint256(8) : 0) | ((ry <= uint64(3)) ? uint256(16) : 0) | ((ry >= uint64(3)) ? uint256(32) : 0) | ((uint64(3) == ry) ? uint256(64) : 0) | ((uint64(3) < ry) ? uint256(128) : 0) | ((uint64(3) > ry) ? uint256(256) : 0) | ((ry == uint64(27)) ? uint256(512) : 0) | ((uint64(27) != ry) ? uint256(1024) : 0); }
    function t_local_cmp_eq() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return (rx == uint64(3)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == uint64(3)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return (ry == uint64(3)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return (rx < uint64(3)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < uint64(3)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_cmp_lt(rx); }
    function t_mem_cmp_lt() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return (ry < uint64(3)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return (uint64(3) > rx) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (uint64(3) > _s_retdirect_cmp_gt()) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_cmp_gt(rx); }
    function t_mem_cmp_gt() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return (uint64(3) > ry) ? 1 : 0; }
}
contract Ci8_0 {
    function _s_retdirect_cmp_all() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_cmp_all(int8 pv) internal view  returns (uint256) { return ((pv == int8(-2)) ? uint256(1) : 0) | ((pv != int8(-2)) ? uint256(2) : 0) | ((pv < int8(-2)) ? uint256(4) : 0) | ((pv > int8(-2)) ? uint256(8) : 0) | ((pv <= int8(-2)) ? uint256(16) : 0) | ((pv >= int8(-2)) ? uint256(32) : 0) | ((int8(-2) == pv) ? uint256(64) : 0) | ((int8(-2) < pv) ? uint256(128) : 0) | ((int8(-2) > pv) ? uint256(256) : 0) | ((pv == int8(-3)) ? uint256(512) : 0) | ((int8(-3) != pv) ? uint256(1024) : 0); }
    function _s_retdirect_cmp_eq() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_cmp_eq(int8 pv) internal view  returns (uint256) { return (pv == int8(-2)) ? 1 : 0; }
    function _s_retdirect_cmp_lt() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_cmp_lt(int8 pv) internal view  returns (uint256) { return (pv < int8(-2)) ? 1 : 0; }
    function _s_retdirect_cmp_gt() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_cmp_gt(int8 pv) internal view  returns (uint256) { return (int8(-2) > pv) ? 1 : 0; }
    function t_local_cmp_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return ((rx == int8(-2)) ? uint256(1) : 0) | ((rx != int8(-2)) ? uint256(2) : 0) | ((rx < int8(-2)) ? uint256(4) : 0) | ((rx > int8(-2)) ? uint256(8) : 0) | ((rx <= int8(-2)) ? uint256(16) : 0) | ((rx >= int8(-2)) ? uint256(32) : 0) | ((int8(-2) == rx) ? uint256(64) : 0) | ((int8(-2) < rx) ? uint256(128) : 0) | ((int8(-2) > rx) ? uint256(256) : 0) | ((rx == int8(-3)) ? uint256(512) : 0) | ((int8(-3) != rx) ? uint256(1024) : 0); }
    function t_retdirect_cmp_all() external view  returns (uint256) { return ((_s_retdirect_cmp_all() == int8(-2)) ? uint256(1) : 0) | ((_s_retdirect_cmp_all() != int8(-2)) ? uint256(2) : 0) | ((_s_retdirect_cmp_all() < int8(-2)) ? uint256(4) : 0) | ((_s_retdirect_cmp_all() > int8(-2)) ? uint256(8) : 0) | ((_s_retdirect_cmp_all() <= int8(-2)) ? uint256(16) : 0) | ((_s_retdirect_cmp_all() >= int8(-2)) ? uint256(32) : 0) | ((int8(-2) == _s_retdirect_cmp_all()) ? uint256(64) : 0) | ((int8(-2) < _s_retdirect_cmp_all()) ? uint256(128) : 0) | ((int8(-2) > _s_retdirect_cmp_all()) ? uint256(256) : 0) | ((_s_retdirect_cmp_all() == int8(-3)) ? uint256(512) : 0) | ((int8(-3) != _s_retdirect_cmp_all()) ? uint256(1024) : 0); }
    function t_param_cmp_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_cmp_all(rx); }
    function t_mem_cmp_all() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return ((ry == int8(-2)) ? uint256(1) : 0) | ((ry != int8(-2)) ? uint256(2) : 0) | ((ry < int8(-2)) ? uint256(4) : 0) | ((ry > int8(-2)) ? uint256(8) : 0) | ((ry <= int8(-2)) ? uint256(16) : 0) | ((ry >= int8(-2)) ? uint256(32) : 0) | ((int8(-2) == ry) ? uint256(64) : 0) | ((int8(-2) < ry) ? uint256(128) : 0) | ((int8(-2) > ry) ? uint256(256) : 0) | ((ry == int8(-3)) ? uint256(512) : 0) | ((int8(-3) != ry) ? uint256(1024) : 0); }
    function t_local_cmp_eq() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return (rx == int8(-2)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == int8(-2)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return (ry == int8(-2)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return (rx < int8(-2)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < int8(-2)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_cmp_lt(rx); }
    function t_mem_cmp_lt() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return (ry < int8(-2)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return (int8(-2) > rx) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (int8(-2) > _s_retdirect_cmp_gt()) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_cmp_gt(rx); }
    function t_mem_cmp_gt() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return (int8(-2) > ry) ? 1 : 0; }
}
contract Ci24_0 {
    function _s_retdirect_cmp_all() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_cmp_all(int24 pv) internal view  returns (uint256) { return ((pv == int24(-2)) ? uint256(1) : 0) | ((pv != int24(-2)) ? uint256(2) : 0) | ((pv < int24(-2)) ? uint256(4) : 0) | ((pv > int24(-2)) ? uint256(8) : 0) | ((pv <= int24(-2)) ? uint256(16) : 0) | ((pv >= int24(-2)) ? uint256(32) : 0) | ((int24(-2) == pv) ? uint256(64) : 0) | ((int24(-2) < pv) ? uint256(128) : 0) | ((int24(-2) > pv) ? uint256(256) : 0) | ((pv == int24(-3)) ? uint256(512) : 0) | ((int24(-3) != pv) ? uint256(1024) : 0); }
    function _s_retdirect_cmp_eq() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_cmp_eq(int24 pv) internal view  returns (uint256) { return (pv == int24(-2)) ? 1 : 0; }
    function _s_retdirect_cmp_lt() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_cmp_lt(int24 pv) internal view  returns (uint256) { return (pv < int24(-2)) ? 1 : 0; }
    function _s_retdirect_cmp_gt() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_cmp_gt(int24 pv) internal view  returns (uint256) { return (int24(-2) > pv) ? 1 : 0; }
    function t_local_cmp_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return ((rx == int24(-2)) ? uint256(1) : 0) | ((rx != int24(-2)) ? uint256(2) : 0) | ((rx < int24(-2)) ? uint256(4) : 0) | ((rx > int24(-2)) ? uint256(8) : 0) | ((rx <= int24(-2)) ? uint256(16) : 0) | ((rx >= int24(-2)) ? uint256(32) : 0) | ((int24(-2) == rx) ? uint256(64) : 0) | ((int24(-2) < rx) ? uint256(128) : 0) | ((int24(-2) > rx) ? uint256(256) : 0) | ((rx == int24(-3)) ? uint256(512) : 0) | ((int24(-3) != rx) ? uint256(1024) : 0); }
    function t_retdirect_cmp_all() external view  returns (uint256) { return ((_s_retdirect_cmp_all() == int24(-2)) ? uint256(1) : 0) | ((_s_retdirect_cmp_all() != int24(-2)) ? uint256(2) : 0) | ((_s_retdirect_cmp_all() < int24(-2)) ? uint256(4) : 0) | ((_s_retdirect_cmp_all() > int24(-2)) ? uint256(8) : 0) | ((_s_retdirect_cmp_all() <= int24(-2)) ? uint256(16) : 0) | ((_s_retdirect_cmp_all() >= int24(-2)) ? uint256(32) : 0) | ((int24(-2) == _s_retdirect_cmp_all()) ? uint256(64) : 0) | ((int24(-2) < _s_retdirect_cmp_all()) ? uint256(128) : 0) | ((int24(-2) > _s_retdirect_cmp_all()) ? uint256(256) : 0) | ((_s_retdirect_cmp_all() == int24(-3)) ? uint256(512) : 0) | ((int24(-3) != _s_retdirect_cmp_all()) ? uint256(1024) : 0); }
    function t_param_cmp_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_cmp_all(rx); }
    function t_mem_cmp_all() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return ((ry == int24(-2)) ? uint256(1) : 0) | ((ry != int24(-2)) ? uint256(2) : 0) | ((ry < int24(-2)) ? uint256(4) : 0) | ((ry > int24(-2)) ? uint256(8) : 0) | ((ry <= int24(-2)) ? uint256(16) : 0) | ((ry >= int24(-2)) ? uint256(32) : 0) | ((int24(-2) == ry) ? uint256(64) : 0) | ((int24(-2) < ry) ? uint256(128) : 0) | ((int24(-2) > ry) ? uint256(256) : 0) | ((ry == int24(-3)) ? uint256(512) : 0) | ((int24(-3) != ry) ? uint256(1024) : 0); }
    function t_local_cmp_eq() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return (rx == int24(-2)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == int24(-2)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return (ry == int24(-2)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return (rx < int24(-2)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < int24(-2)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_cmp_lt(rx); }
    function t_mem_cmp_lt() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return (ry < int24(-2)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return (int24(-2) > rx) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (int24(-2) > _s_retdirect_cmp_gt()) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_cmp_gt(rx); }
    function t_mem_cmp_gt() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return (int24(-2) > ry) ? 1 : 0; }
}
