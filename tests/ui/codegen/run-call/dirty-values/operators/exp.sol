//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_exp_exponent => 134217728
//@ run-call: Cu8_0::t_retdirect_exp_exponent => 134217728
//@ run-call: Cu8_0::t_ret_exp_exponent => 134217728
//@ run-call: Cu8_0::t_param_exp_exponent => 134217728
//@ run-call: Cu8_0::t_copy_exp_exponent => 134217728
//@ run-call: Cu8_0::t_ternary_exp_exponent => 134217728
//@ run-call: Cu8_0::t_ifjoin_exp_exponent => 134217728
//@ run-call: Cu8_0::t_loopjoin_exp_exponent => 134217728
//@ run-call: Cu8_0::t_tuple_exp_exponent => 134217728
//@ run-call: Cu8_0::t_modifier_exp_exponent => 134217728
//@ run-call: Cu8_0::t_ptr_exp_exponent => 134217728
//@ run-call: Cu8_0::t_mem_exp_exponent => 134217728
//@ run-call-fail: Cu8_0::t_local_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_retdirect_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ret_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_param_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_copy_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ternary_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ifjoin_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_loopjoin_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_tuple_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_modifier_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ptr_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_mem_exp_exponent_narrow => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Cu8_0::t_local_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_retdirect_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_ret_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_param_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_copy_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_ternary_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_ifjoin_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_loopjoin_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_tuple_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_modifier_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_ptr_exp_exponent_unc => 187
//@ run-call: Cu8_0::t_mem_exp_exponent_unc => 187
//@ run-call-fail: Cu8_0::t_local_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_retdirect_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ret_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_param_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_copy_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ternary_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ifjoin_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_loopjoin_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_tuple_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_modifier_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ptr_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_mem_exp_base => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Cu8_1::t_local_exp_base_unc => 227
//@ run-call: Cu8_1::t_retdirect_exp_base_unc => 227
//@ run-call: Cu8_1::t_ret_exp_base_unc => 227
//@ run-call: Cu8_1::t_param_exp_base_unc => 227
//@ run-call: Cu8_1::t_copy_exp_base_unc => 227
//@ run-call: Cu8_1::t_ternary_exp_base_unc => 227
//@ run-call: Cu8_1::t_ifjoin_exp_base_unc => 227
//@ run-call: Cu8_1::t_loopjoin_exp_base_unc => 227
//@ run-call: Cu8_1::t_tuple_exp_base_unc => 227
//@ run-call: Cu8_1::t_modifier_exp_base_unc => 227
//@ run-call: Cu8_1::t_ptr_exp_base_unc => 227
//@ run-call: Cu8_1::t_mem_exp_base_unc => 227
//@ run-call: Cu64_0::t_local_exp_exponent => 134217728
//@ run-call: Cu64_0::t_retdirect_exp_exponent => 134217728
//@ run-call: Cu64_0::t_param_exp_exponent => 134217728
//@ run-call: Cu64_0::t_mem_exp_exponent => 134217728
//@ run-call: Cu64_0::t_local_exp_exponent_narrow => 7625597484987
//@ run-call: Cu64_0::t_retdirect_exp_exponent_narrow => 7625597484987
//@ run-call: Cu64_0::t_param_exp_exponent_narrow => 7625597484987
//@ run-call: Cu64_0::t_mem_exp_exponent_narrow => 7625597484987
//@ run-call: Cu64_0::t_local_exp_exponent_unc => 7625597484987
//@ run-call: Cu64_0::t_retdirect_exp_exponent_unc => 7625597484987
//@ run-call: Cu64_0::t_param_exp_exponent_unc => 7625597484987
//@ run-call: Cu64_0::t_mem_exp_exponent_unc => 7625597484987
//@ run-call: Cu64_0::t_local_exp_base => 729
//@ run-call: Cu64_0::t_retdirect_exp_base => 729
//@ run-call: Cu64_0::t_param_exp_base => 729
//@ run-call: Cu64_0::t_mem_exp_base => 729
//@ run-call: Cu64_0::t_local_exp_base_unc => 19683
//@ run-call: Cu64_0::t_retdirect_exp_base_unc => 19683
//@ run-call: Cu64_0::t_param_exp_base_unc => 19683
//@ run-call: Cu64_0::t_mem_exp_base_unc => 19683
//@ run-call: Ci8_0::t_local_exp_base => 9
//@ run-call: Ci8_0::t_retdirect_exp_base => 9
//@ run-call: Ci8_0::t_param_exp_base => 9
//@ run-call: Ci8_0::t_mem_exp_base => 9
//@ run-call: Ci8_0::t_local_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci8_0::t_retdirect_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci8_0::t_param_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci8_0::t_mem_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci24_0::t_local_exp_base => 9
//@ run-call: Ci24_0::t_retdirect_exp_base => 9
//@ run-call: Ci24_0::t_param_exp_base => 9
//@ run-call: Ci24_0::t_mem_exp_base => 9
//@ run-call: Ci24_0::t_local_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci24_0::t_retdirect_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci24_0::t_param_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909
//@ run-call: Ci24_0::t_mem_exp_base_unc => 115792089237316195423570985008687907853269984665640564039457584007913129639909

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
    function _s_retdirect_exp_exponent() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_exp_exponent() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_exp_exponent(uint8 pv) internal view  returns (uint256) { return uint256(2) ** pv; }
    uint256 _res_modifier_exp_exponent;
    modifier _m_modifier_exp_exponent(uint8 pv) { _res_modifier_exp_exponent = uint256(2) ** pv; _; }
    function _e_modifier_exp_exponent(uint8 rd) internal _m_modifier_exp_exponent(rd)  returns (uint256) { return _res_modifier_exp_exponent; }
    function _w_ptr_exp_exponent() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_exp_exponent_narrow() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_exp_exponent_narrow() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_exp_exponent_narrow(uint8 pv) internal view  returns (uint256) { return uint256(uint8(3) ** pv); }
    uint256 _res_modifier_exp_exponent_narrow;
    modifier _m_modifier_exp_exponent_narrow(uint8 pv) { _res_modifier_exp_exponent_narrow = uint256(uint8(3) ** pv); _; }
    function _e_modifier_exp_exponent_narrow(uint8 rd) internal _m_modifier_exp_exponent_narrow(rd)  returns (uint256) { return _res_modifier_exp_exponent_narrow; }
    function _w_ptr_exp_exponent_narrow() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_exp_exponent_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_exp_exponent_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_exp_exponent_unc(uint8 pv) internal view  returns (uint256) { unchecked { return uint256(uint8(3) ** pv); } }
    uint256 _res_modifier_exp_exponent_unc;
    modifier _m_modifier_exp_exponent_unc(uint8 pv) { unchecked { _res_modifier_exp_exponent_unc = uint256(uint8(3) ** pv); } _; }
    function _e_modifier_exp_exponent_unc(uint8 rd) internal _m_modifier_exp_exponent_unc(rd)  returns (uint256) { return _res_modifier_exp_exponent_unc; }
    function _w_ptr_exp_exponent_unc() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_exp_base() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_exp_base() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_exp_base(uint8 pv) internal view  returns (uint256) { return uint256(pv ** 2); }
    function t_local_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(2) ** rx; }
    function t_retdirect_exp_exponent() external view  returns (uint256) { return uint256(2) ** _s_retdirect_exp_exponent(); }
    function t_ret_exp_exponent() external view  returns (uint256) { uint8 rx = _s_ret_exp_exponent(); return uint256(2) ** rx; }
    function t_param_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_exp_exponent(rx); }
    function t_copy_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(2) ** ry; }
    function t_ternary_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(2) ** ry; }
    function t_ifjoin_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(2) ** ry; }
    function t_loopjoin_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(2) ** ry; }
    function t_tuple_exp_exponent() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(2) ** ry; }
    function t_modifier_exp_exponent() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_exp_exponent(rx); }
    function t_ptr_exp_exponent() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_exp_exponent; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(2) ** ry; }
    function t_mem_exp_exponent() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(2) ** ry; }
    function t_local_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(3) ** rx); }
    function t_retdirect_exp_exponent_narrow() external view  returns (uint256) { return uint256(uint8(3) ** _s_retdirect_exp_exponent_narrow()); }
    function t_ret_exp_exponent_narrow() external view  returns (uint256) { uint8 rx = _s_ret_exp_exponent_narrow(); return uint256(uint8(3) ** rx); }
    function t_param_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_exp_exponent_narrow(rx); }
    function t_copy_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(3) ** ry); }
    function t_ternary_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(3) ** ry); }
    function t_ifjoin_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(3) ** ry); }
    function t_loopjoin_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(3) ** ry); }
    function t_tuple_exp_exponent_narrow() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(3) ** ry); }
    function t_modifier_exp_exponent_narrow() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_exp_exponent_narrow(rx); }
    function t_ptr_exp_exponent_narrow() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_exp_exponent_narrow; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(3) ** ry); }
    function t_mem_exp_exponent_narrow() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(3) ** ry); }
    function t_local_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(3) ** rx); } }
    function t_retdirect_exp_exponent_unc() external view  returns (uint256) { unchecked { return uint256(uint8(3) ** _s_retdirect_exp_exponent_unc()); } }
    function t_ret_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx = _s_ret_exp_exponent_unc(); return uint256(uint8(3) ** rx); } }
    function t_param_exp_exponent_unc() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_exp_exponent_unc(rx); }
    function t_copy_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(3) ** ry); } }
    function t_ternary_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(3) ** ry); } }
    function t_ifjoin_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(3) ** ry); } }
    function t_loopjoin_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(3) ** ry); } }
    function t_tuple_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(3) ** ry); } }
    function t_modifier_exp_exponent_unc() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_exp_exponent_unc(rx); }
    function t_ptr_exp_exponent_unc() external view  returns (uint256) { unchecked { function() internal pure returns (uint256) rp = _w_ptr_exp_exponent_unc; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(3) ** ry); } }
    function t_mem_exp_exponent_unc() external view  returns (uint256) { unchecked { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(3) ** ry); } }
    function t_local_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx ** 2); }
    function t_retdirect_exp_base() external view  returns (uint256) { return uint256(_s_retdirect_exp_base() ** 2); }
    function t_ret_exp_base() external view  returns (uint256) { uint8 rx = _s_ret_exp_base(); return uint256(rx ** 2); }
    function t_param_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_exp_base(rx); }
}
contract Cu8_1 {
    uint256 _res_modifier_exp_base;
    modifier _m_modifier_exp_base(uint8 pv) { _res_modifier_exp_base = uint256(pv ** 2); _; }
    function _e_modifier_exp_base(uint8 rd) internal _m_modifier_exp_base(rd)  returns (uint256) { return _res_modifier_exp_base; }
    function _w_ptr_exp_base() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_exp_base_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_exp_base_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_exp_base_unc(uint8 pv) internal view  returns (uint256) { unchecked { return uint256(pv ** 3); } }
    uint256 _res_modifier_exp_base_unc;
    modifier _m_modifier_exp_base_unc(uint8 pv) { unchecked { _res_modifier_exp_base_unc = uint256(pv ** 3); } _; }
    function _e_modifier_exp_base_unc(uint8 rd) internal _m_modifier_exp_base_unc(rd)  returns (uint256) { return _res_modifier_exp_base_unc; }
    function _w_ptr_exp_base_unc() internal pure returns (uint256) { return 0xabcd1b; }
    function t_copy_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry ** 2); }
    function t_ternary_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry ** 2); }
    function t_ifjoin_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry ** 2); }
    function t_loopjoin_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry ** 2); }
    function t_tuple_exp_base() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry ** 2); }
    function t_modifier_exp_base() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_exp_base(rx); }
    function t_ptr_exp_base() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_exp_base; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry ** 2); }
    function t_mem_exp_base() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry ** 2); }
    function t_local_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx ** 3); } }
    function t_retdirect_exp_base_unc() external view  returns (uint256) { unchecked { return uint256(_s_retdirect_exp_base_unc() ** 3); } }
    function t_ret_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx = _s_ret_exp_base_unc(); return uint256(rx ** 3); } }
    function t_param_exp_base_unc() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_exp_base_unc(rx); }
    function t_copy_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry ** 3); } }
    function t_ternary_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry ** 3); } }
    function t_ifjoin_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry ** 3); } }
    function t_loopjoin_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry ** 3); } }
    function t_tuple_exp_base_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry ** 3); } }
    function t_modifier_exp_base_unc() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_exp_base_unc(rx); }
    function t_ptr_exp_base_unc() external view  returns (uint256) { unchecked { function() internal pure returns (uint256) rp = _w_ptr_exp_base_unc; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry ** 3); } }
    function t_mem_exp_base_unc() external view  returns (uint256) { unchecked { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry ** 3); } }
}
contract Cu64_0 {
    function _s_retdirect_exp_exponent() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_exp_exponent(uint64 pv) internal view  returns (uint256) { return uint256(2) ** pv; }
    function _s_retdirect_exp_exponent_narrow() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_exp_exponent_narrow(uint64 pv) internal view  returns (uint256) { return uint256(uint64(3) ** pv); }
    function _s_retdirect_exp_exponent_unc() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_exp_exponent_unc(uint64 pv) internal view  returns (uint256) { unchecked { return uint256(uint64(3) ** pv); } }
    function _s_retdirect_exp_base() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_exp_base(uint64 pv) internal view  returns (uint256) { return uint256(pv ** 2); }
    function _s_retdirect_exp_base_unc() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_exp_base_unc(uint64 pv) internal view  returns (uint256) { unchecked { return uint256(pv ** 3); } }
    function t_local_exp_exponent() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(2) ** rx; }
    function t_retdirect_exp_exponent() external view  returns (uint256) { return uint256(2) ** _s_retdirect_exp_exponent(); }
    function t_param_exp_exponent() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_exp_exponent(rx); }
    function t_mem_exp_exponent() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(2) ** ry; }
    function t_local_exp_exponent_narrow() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(3) ** rx); }
    function t_retdirect_exp_exponent_narrow() external view  returns (uint256) { return uint256(uint64(3) ** _s_retdirect_exp_exponent_narrow()); }
    function t_param_exp_exponent_narrow() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_exp_exponent_narrow(rx); }
    function t_mem_exp_exponent_narrow() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(3) ** ry); }
    function t_local_exp_exponent_unc() external view  returns (uint256) { unchecked { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(3) ** rx); } }
    function t_retdirect_exp_exponent_unc() external view  returns (uint256) { unchecked { return uint256(uint64(3) ** _s_retdirect_exp_exponent_unc()); } }
    function t_param_exp_exponent_unc() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_exp_exponent_unc(rx); }
    function t_mem_exp_exponent_unc() external view  returns (uint256) { unchecked { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(3) ** ry); } }
    function t_local_exp_base() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx ** 2); }
    function t_retdirect_exp_base() external view  returns (uint256) { return uint256(_s_retdirect_exp_base() ** 2); }
    function t_param_exp_base() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_exp_base(rx); }
    function t_mem_exp_base() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry ** 2); }
    function t_local_exp_base_unc() external view  returns (uint256) { unchecked { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx ** 3); } }
    function t_retdirect_exp_base_unc() external view  returns (uint256) { unchecked { return uint256(_s_retdirect_exp_base_unc() ** 3); } }
    function t_param_exp_base_unc() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_exp_base_unc(rx); }
    function t_mem_exp_base_unc() external view  returns (uint256) { unchecked { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry ** 3); } }
}
contract Ci8_0 {
    function _s_retdirect_exp_base() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_exp_base(int8 pv) internal view  returns (uint256) { return uint256(int256(pv ** 2)); }
    function _s_retdirect_exp_base_unc() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_exp_base_unc(int8 pv) internal view  returns (uint256) { unchecked { return uint256(int256(pv ** 3)); } }
    function t_local_exp_base() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx ** 2)); }
    function t_retdirect_exp_base() external view  returns (uint256) { return uint256(int256(_s_retdirect_exp_base() ** 2)); }
    function t_param_exp_base() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_exp_base(rx); }
    function t_mem_exp_base() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry ** 2)); }
    function t_local_exp_base_unc() external view  returns (uint256) { unchecked { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx ** 3)); } }
    function t_retdirect_exp_base_unc() external view  returns (uint256) { unchecked { return uint256(int256(_s_retdirect_exp_base_unc() ** 3)); } }
    function t_param_exp_base_unc() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_exp_base_unc(rx); }
    function t_mem_exp_base_unc() external view  returns (uint256) { unchecked { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry ** 3)); } }
}
contract Ci24_0 {
    function _s_retdirect_exp_base() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_exp_base(int24 pv) internal view  returns (uint256) { return uint256(int256(pv ** 2)); }
    function _s_retdirect_exp_base_unc() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_exp_base_unc(int24 pv) internal view  returns (uint256) { unchecked { return uint256(int256(pv ** 3)); } }
    function t_local_exp_base() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx ** 2)); }
    function t_retdirect_exp_base() external view  returns (uint256) { return uint256(int256(_s_retdirect_exp_base() ** 2)); }
    function t_param_exp_base() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_exp_base(rx); }
    function t_mem_exp_base() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry ** 2)); }
    function t_local_exp_base_unc() external view  returns (uint256) { unchecked { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx ** 3)); } }
    function t_retdirect_exp_base_unc() external view  returns (uint256) { unchecked { return uint256(int256(_s_retdirect_exp_base_unc() ** 3)); } }
    function t_param_exp_base_unc() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_exp_base_unc(rx); }
    function t_mem_exp_base_unc() external view  returns (uint256) { unchecked { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry ** 3)); } }
}
