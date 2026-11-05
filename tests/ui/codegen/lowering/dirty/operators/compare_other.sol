//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_eq => 0
//@ run-call: Cbool_0::t_retdirect_eq => 0
//@ run-call: Cbool_0::t_param_eq => 0
//@ run-call: Cbool_0::t_mem_eq => 0
//@ run-call: Cbool_0::t_local_eq_valid => 1
//@ run-call: Cbool_0::t_retdirect_eq_valid => 1
//@ run-call: Cbool_0::t_param_eq_valid => 1
//@ run-call: Cbool_0::t_mem_eq_valid => 1
//@ run-call: Cbool_0::t_local_ne => 1
//@ run-call: Cbool_0::t_retdirect_ne => 1
//@ run-call: Cbool_0::t_param_ne => 1
//@ run-call: Cbool_0::t_mem_ne => 1
//@ run-call-fail: Cen_0::t_local_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_cmp_eq => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_cmp_ne => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_cmp_valid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_cmp => 0
//@ run-call: Cud_0::t_retdirect_cmp => 0
//@ run-call: Cud_0::t_param_cmp => 0
//@ run-call: Cud_0::t_mem_cmp => 0
//@ run-call: Cud_0::t_local_cmp_valid => 1
//@ run-call: Cud_0::t_retdirect_cmp_valid => 1
//@ run-call: Cud_0::t_param_cmp_valid => 1
//@ run-call: Cud_0::t_mem_cmp_valid => 1

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

contract Cbool_0 {
    function _s_retdirect_eq() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_eq(bool pv) internal view  returns (uint256) { return (pv == false) ? 1 : 0; }
    function _s_retdirect_eq_valid() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_eq_valid(bool pv) internal view  returns (uint256) { return (pv == true) ? 1 : 0; }
    function _s_retdirect_ne() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ne(bool pv) internal view  returns (uint256) { return (pv != false) ? 1 : 0; }
    function t_local_eq() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (rx == false) ? 1 : 0; }
    function t_retdirect_eq() external view  returns (uint256) { return (_s_retdirect_eq() == false) ? 1 : 0; }
    function t_param_eq() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_eq(rx); }
    function t_mem_eq() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (ry == false) ? 1 : 0; }
    function t_local_eq_valid() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (rx == true) ? 1 : 0; }
    function t_retdirect_eq_valid() external view  returns (uint256) { return (_s_retdirect_eq_valid() == true) ? 1 : 0; }
    function t_param_eq_valid() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_eq_valid(rx); }
    function t_mem_eq_valid() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (ry == true) ? 1 : 0; }
    function t_local_ne() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (rx != false) ? 1 : 0; }
    function t_retdirect_ne() external view  returns (uint256) { return (_s_retdirect_ne() != false) ? 1 : 0; }
    function t_param_ne() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ne(rx); }
    function t_mem_ne() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (ry != false) ? 1 : 0; }
}
contract Cen_0 {
    function _s_retdirect_cmp_eq() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_cmp_eq() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_cmp_eq(E pv) internal view  returns (uint256) { return (pv == E.A) ? 1 : 0; }
    uint256 _res_modifier_cmp_eq;
    modifier _m_modifier_cmp_eq(E pv) { _res_modifier_cmp_eq = (pv == E.A) ? 1 : 0; _; }
    function _e_modifier_cmp_eq(E rd) internal _m_modifier_cmp_eq(rd)  returns (uint256) { return _res_modifier_cmp_eq; }
    function _w_ptr_cmp_eq() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_cmp_ne() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_cmp_ne() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_cmp_ne(E pv) internal view  returns (uint256) { return (pv != E.A) ? 1 : 0; }
    uint256 _res_modifier_cmp_ne;
    modifier _m_modifier_cmp_ne(E pv) { _res_modifier_cmp_ne = (pv != E.A) ? 1 : 0; _; }
    function _e_modifier_cmp_ne(E rd) internal _m_modifier_cmp_ne(rd)  returns (uint256) { return _res_modifier_cmp_ne; }
    function _w_ptr_cmp_ne() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_cmp_valid() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_cmp_valid() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_cmp_valid(E pv) internal view  returns (uint256) { return (pv == E.B) ? 1 : 0; }
    uint256 _res_modifier_cmp_valid;
    modifier _m_modifier_cmp_valid(E pv) { _res_modifier_cmp_valid = (pv == E.B) ? 1 : 0; _; }
    function _e_modifier_cmp_valid(E rd) internal _m_modifier_cmp_valid(rd)  returns (uint256) { return _res_modifier_cmp_valid; }
    function _w_ptr_cmp_valid() internal pure returns (uint256) { return 0x101; }
    function t_local_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return (rx == E.A) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == E.A) ? 1 : 0; }
    function t_ret_cmp_eq() external view  returns (uint256) { E rx = _s_ret_cmp_eq(); return (rx == E.A) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_cmp_eq(rx); }
    function t_copy_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return (ry == E.A) ? 1 : 0; }
    function t_ternary_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return (ry == E.A) ? 1 : 0; }
    function t_ifjoin_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == E.A) ? 1 : 0; }
    function t_loopjoin_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == E.A) ? 1 : 0; }
    function t_tuple_cmp_eq() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == E.A) ? 1 : 0; }
    function t_modifier_cmp_eq() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_cmp_eq(rx); }
    function t_ptr_cmp_eq() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_eq; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return (ry == E.A) ? 1 : 0; }
    function t_mem_cmp_eq() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return (ry == E.A) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return (rx != E.A) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (_s_retdirect_cmp_ne() != E.A) ? 1 : 0; }
    function t_ret_cmp_ne() external view  returns (uint256) { E rx = _s_ret_cmp_ne(); return (rx != E.A) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_cmp_ne(rx); }
    function t_copy_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return (ry != E.A) ? 1 : 0; }
    function t_ternary_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return (ry != E.A) ? 1 : 0; }
    function t_ifjoin_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return (ry != E.A) ? 1 : 0; }
    function t_loopjoin_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry != E.A) ? 1 : 0; }
    function t_tuple_cmp_ne() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry != E.A) ? 1 : 0; }
    function t_modifier_cmp_ne() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_cmp_ne(rx); }
    function t_ptr_cmp_ne() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_ne; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return (ry != E.A) ? 1 : 0; }
    function t_mem_cmp_ne() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return (ry != E.A) ? 1 : 0; }
    function t_local_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return (rx == E.B) ? 1 : 0; }
    function t_retdirect_cmp_valid() external view  returns (uint256) { return (_s_retdirect_cmp_valid() == E.B) ? 1 : 0; }
    function t_ret_cmp_valid() external view  returns (uint256) { E rx = _s_ret_cmp_valid(); return (rx == E.B) ? 1 : 0; }
    function t_param_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_cmp_valid(rx); }
    function t_copy_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return (ry == E.B) ? 1 : 0; }
    function t_ternary_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return (ry == E.B) ? 1 : 0; }
    function t_ifjoin_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == E.B) ? 1 : 0; }
    function t_loopjoin_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == E.B) ? 1 : 0; }
    function t_tuple_cmp_valid() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == E.B) ? 1 : 0; }
    function t_modifier_cmp_valid() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_cmp_valid(rx); }
    function t_ptr_cmp_valid() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_valid; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return (ry == E.B) ? 1 : 0; }
    function t_mem_cmp_valid() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return (ry == E.B) ? 1 : 0; }
}
contract Cud_0 {
    function _s_retdirect_cmp() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp(U pv) internal view  returns (uint256) { return (U.unwrap(pv) == U.unwrap(U.wrap(3))) ? 1 : 0; }
    function _s_retdirect_cmp_valid() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_cmp_valid(U pv) internal view  returns (uint256) { return (U.unwrap(pv) == U.unwrap(U.wrap(27))) ? 1 : 0; }
    function t_local_cmp() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return (U.unwrap(rx) == U.unwrap(U.wrap(3))) ? 1 : 0; }
    function t_retdirect_cmp() external view  returns (uint256) { return (U.unwrap(_s_retdirect_cmp()) == U.unwrap(U.wrap(3))) ? 1 : 0; }
    function t_param_cmp() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_cmp(rx); }
    function t_mem_cmp() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return (U.unwrap(ry) == U.unwrap(U.wrap(3))) ? 1 : 0; }
    function t_local_cmp_valid() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return (U.unwrap(rx) == U.unwrap(U.wrap(27))) ? 1 : 0; }
    function t_retdirect_cmp_valid() external view  returns (uint256) { return (U.unwrap(_s_retdirect_cmp_valid()) == U.unwrap(U.wrap(27))) ? 1 : 0; }
    function t_param_cmp_valid() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_cmp_valid(rx); }
    function t_mem_cmp_valid() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return (U.unwrap(ry) == U.unwrap(U.wrap(27))) ? 1 : 0; }
}
