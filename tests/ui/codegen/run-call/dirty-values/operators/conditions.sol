//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_ternary => 11
//@ run-call: Cbool_0::t_retdirect_ternary => 11
//@ run-call: Cbool_0::t_param_ternary => 11
//@ run-call: Cbool_0::t_mem_ternary => 11
//@ run-call: Cbool_0::t_local_if => 11
//@ run-call: Cbool_0::t_retdirect_if => 11
//@ run-call: Cbool_0::t_param_if => 11
//@ run-call: Cbool_0::t_mem_if => 11
//@ run-call: Cbool_0::t_local_require => 33
//@ run-call: Cbool_0::t_retdirect_require => 33
//@ run-call: Cbool_0::t_param_require => 33
//@ run-call: Cbool_0::t_mem_require => 33
//@ run-call: Cbool_0::t_local_assert => 44
//@ run-call: Cbool_0::t_retdirect_assert => 44
//@ run-call: Cbool_0::t_param_assert => 44
//@ run-call: Cbool_0::t_mem_assert => 44
//@ run-call: Cbool_0::t_local_while => 55
//@ run-call: Cbool_0::t_retdirect_while => 55
//@ run-call: Cbool_0::t_param_while => 55
//@ run-call: Cbool_0::t_mem_while => 55
//@ run-call: Cbool_0::t_local_require_msg => 66
//@ run-call: Cbool_0::t_retdirect_require_msg => 66
//@ run-call: Cbool_0::t_param_require_msg => 66
//@ run-call: Cbool_0::t_mem_require_msg => 66
//@ run-call: Cbool_0::t_local_dowhile => 77
//@ run-call: Cbool_0::t_retdirect_dowhile => 77
//@ run-call: Cbool_0::t_param_dowhile => 77
//@ run-call: Cbool_0::t_mem_dowhile => 77
//@ run-call-fail: Cen_0::t_local_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_switch_if => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

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
    function _s_retdirect_ternary() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ternary(bool pv) internal view  returns (uint256) { return pv ? 11 : 22; }
    function _s_retdirect_if() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_if(bool pv) internal view  returns (uint256) { if (pv) { return 11; } return 22; }
    function _s_retdirect_require() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_require(bool pv) internal view  returns (uint256) { require(pv); return 33; }
    function _s_retdirect_assert() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_assert(bool pv) internal view  returns (uint256) { assert(pv); return 44; }
    function _s_retdirect_while() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_while(bool pv) internal view  returns (uint256) { while (pv) { break; } return 55; }
    function _s_retdirect_require_msg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_require_msg(bool pv) internal view  returns (uint256) { require(pv, "m"); return 66; }
    function _s_retdirect_dowhile() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_dowhile(bool pv) internal view  returns (uint256) { uint256 n; do { n++; } while (pv && n < 2); return 77; }
    function t_local_ternary() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return rx ? 11 : 22; }
    function t_retdirect_ternary() external view  returns (uint256) { return _s_retdirect_ternary() ? 11 : 22; }
    function t_param_ternary() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ternary(rx); }
    function t_mem_ternary() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return ry ? 11 : 22; }
    function t_local_if() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } if (rx) { return 11; } return 22; }
    function t_retdirect_if() external view  returns (uint256) { if (_s_retdirect_if()) { return 11; } return 22; }
    function t_param_if() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_if(rx); }
    function t_mem_if() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; if (ry) { return 11; } return 22; }
    function t_local_require() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } require(rx); return 33; }
    function t_retdirect_require() external view  returns (uint256) { require(_s_retdirect_require()); return 33; }
    function t_param_require() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_require(rx); }
    function t_mem_require() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; require(ry); return 33; }
    function t_local_assert() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } assert(rx); return 44; }
    function t_retdirect_assert() external view  returns (uint256) { assert(_s_retdirect_assert()); return 44; }
    function t_param_assert() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_assert(rx); }
    function t_mem_assert() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; assert(ry); return 44; }
    function t_local_while() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } while (rx) { break; } return 55; }
    function t_retdirect_while() external view  returns (uint256) { while (_s_retdirect_while()) { break; } return 55; }
    function t_param_while() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_while(rx); }
    function t_mem_while() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; while (ry) { break; } return 55; }
    function t_local_require_msg() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } require(rx, "m"); return 66; }
    function t_retdirect_require_msg() external view  returns (uint256) { require(_s_retdirect_require_msg(), "m"); return 66; }
    function t_param_require_msg() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_require_msg(rx); }
    function t_mem_require_msg() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; require(ry, "m"); return 66; }
    function t_local_dowhile() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } uint256 n; do { n++; } while (rx && n < 2); return 77; }
    function t_retdirect_dowhile() external view  returns (uint256) { uint256 n; do { n++; } while (_s_retdirect_dowhile() && n < 2); return 77; }
    function t_param_dowhile() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_dowhile(rx); }
    function t_mem_dowhile() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; uint256 n; do { n++; } while (ry && n < 2); return 77; }
}
contract Cen_0 {
    function _s_retdirect_switch_if() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_switch_if() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_switch_if(E pv) internal view  returns (uint256) { if (pv == E.B) { return 11; } return 22; }
    function _w_ptr_switch_if() internal pure returns (uint256) { return 0x101; }
    function t_local_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } if (rx == E.B) { return 11; } return 22; }
    function t_retdirect_switch_if() external view  returns (uint256) { if (_s_retdirect_switch_if() == E.B) { return 11; } return 22; }
    function t_ret_switch_if() external view  returns (uint256) { E rx = _s_ret_switch_if(); if (rx == E.B) { return 11; } return 22; }
    function t_param_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_switch_if(rx); }
    function t_copy_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; if (ry == E.B) { return 11; } return 22; }
    function t_ternary_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; if (ry == E.B) { return 11; } return 22; }
    function t_ifjoin_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } if (ry == E.B) { return 11; } return 22; }
    function t_loopjoin_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } if (ry == E.B) { return 11; } return 22; }
    function t_tuple_switch_if() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; if (ry == E.B) { return 11; } return 22; }
    function t_ptr_switch_if() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_switch_if; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); if (ry == E.B) { return 11; } return 22; }
    function t_mem_switch_if() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; if (ry == E.B) { return 11; } return 22; }
}
