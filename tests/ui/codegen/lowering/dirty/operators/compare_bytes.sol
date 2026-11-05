//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_cmp_eq => 0
//@ run-call: Cb1_0::t_retdirect_cmp_eq => 0
//@ run-call: Cb1_0::t_param_cmp_eq => 0
//@ run-call: Cb1_0::t_mem_cmp_eq => 0
//@ run-call: Cb1_0::t_local_cmp_ne => 1
//@ run-call: Cb1_0::t_retdirect_cmp_ne => 1
//@ run-call: Cb1_0::t_param_cmp_ne => 1
//@ run-call: Cb1_0::t_mem_cmp_ne => 1
//@ run-call: Cb1_0::t_local_cmp_lt => 1
//@ run-call: Cb1_0::t_retdirect_cmp_lt => 1
//@ run-call: Cb1_0::t_param_cmp_lt => 1
//@ run-call: Cb1_0::t_mem_cmp_lt => 1
//@ run-call: Cb1_0::t_local_cmp_gt => 0
//@ run-call: Cb1_0::t_retdirect_cmp_gt => 0
//@ run-call: Cb1_0::t_param_cmp_gt => 0
//@ run-call: Cb1_0::t_mem_cmp_gt => 0
//@ run-call: Cb1_0::t_local_cmp_valid => 1
//@ run-call: Cb1_0::t_retdirect_cmp_valid => 1
//@ run-call: Cb1_0::t_param_cmp_valid => 1
//@ run-call: Cb1_0::t_mem_cmp_valid => 1
//@ run-call: Cb4_0::t_local_cmp_eq => 0
//@ run-call: Cb4_0::t_retdirect_cmp_eq => 0
//@ run-call: Cb4_0::t_ret_cmp_eq => 0
//@ run-call: Cb4_0::t_param_cmp_eq => 0
//@ run-call: Cb4_0::t_copy_cmp_eq => 0
//@ run-call: Cb4_0::t_ternary_cmp_eq => 0
//@ run-call: Cb4_0::t_ifjoin_cmp_eq => 0
//@ run-call: Cb4_0::t_loopjoin_cmp_eq => 0
//@ run-call: Cb4_0::t_tuple_cmp_eq => 0
//@ run-call: Cb4_0::t_modifier_cmp_eq => 0
//@ run-call: Cb4_0::t_ptr_cmp_eq => 0
//@ run-call: Cb4_0::t_mem_cmp_eq => 0
//@ run-call: Cb4_0::t_local_cmp_ne => 1
//@ run-call: Cb4_0::t_retdirect_cmp_ne => 1
//@ run-call: Cb4_0::t_ret_cmp_ne => 1
//@ run-call: Cb4_0::t_param_cmp_ne => 1
//@ run-call: Cb4_0::t_copy_cmp_ne => 1
//@ run-call: Cb4_0::t_ternary_cmp_ne => 1
//@ run-call: Cb4_0::t_ifjoin_cmp_ne => 1
//@ run-call: Cb4_0::t_loopjoin_cmp_ne => 1
//@ run-call: Cb4_0::t_tuple_cmp_ne => 1
//@ run-call: Cb4_0::t_modifier_cmp_ne => 1
//@ run-call: Cb4_0::t_ptr_cmp_ne => 1
//@ run-call: Cb4_0::t_mem_cmp_ne => 1
//@ run-call: Cb4_0::t_local_cmp_lt => 0
//@ run-call: Cb4_0::t_retdirect_cmp_lt => 0
//@ run-call: Cb4_0::t_ret_cmp_lt => 0
//@ run-call: Cb4_0::t_param_cmp_lt => 0
//@ run-call: Cb4_0::t_copy_cmp_lt => 0
//@ run-call: Cb4_0::t_ternary_cmp_lt => 0
//@ run-call: Cb4_0::t_ifjoin_cmp_lt => 0
//@ run-call: Cb4_0::t_loopjoin_cmp_lt => 0
//@ run-call: Cb4_0::t_tuple_cmp_lt => 0
//@ run-call: Cb4_0::t_modifier_cmp_lt => 0
//@ run-call: Cb4_0::t_ptr_cmp_lt => 0
//@ run-call: Cb4_0::t_mem_cmp_lt => 0
//@ run-call: Cb4_0::t_local_cmp_gt => 1
//@ run-call: Cb4_0::t_retdirect_cmp_gt => 1
//@ run-call: Cb4_0::t_ret_cmp_gt => 1
//@ run-call: Cb4_0::t_param_cmp_gt => 1
//@ run-call: Cb4_1::t_copy_cmp_gt => 1
//@ run-call: Cb4_1::t_ternary_cmp_gt => 1
//@ run-call: Cb4_1::t_ifjoin_cmp_gt => 1
//@ run-call: Cb4_1::t_loopjoin_cmp_gt => 1
//@ run-call: Cb4_1::t_tuple_cmp_gt => 1
//@ run-call: Cb4_1::t_modifier_cmp_gt => 1
//@ run-call: Cb4_1::t_ptr_cmp_gt => 1
//@ run-call: Cb4_1::t_mem_cmp_gt => 1
//@ run-call: Cb4_1::t_local_cmp_valid => 1
//@ run-call: Cb4_1::t_retdirect_cmp_valid => 1
//@ run-call: Cb4_1::t_ret_cmp_valid => 1
//@ run-call: Cb4_1::t_param_cmp_valid => 1
//@ run-call: Cb4_1::t_copy_cmp_valid => 1
//@ run-call: Cb4_1::t_ternary_cmp_valid => 1
//@ run-call: Cb4_1::t_ifjoin_cmp_valid => 1
//@ run-call: Cb4_1::t_loopjoin_cmp_valid => 1
//@ run-call: Cb4_1::t_tuple_cmp_valid => 1
//@ run-call: Cb4_1::t_modifier_cmp_valid => 1
//@ run-call: Cb4_1::t_ptr_cmp_valid => 1
//@ run-call: Cb4_1::t_mem_cmp_valid => 1
//@ run-call: Cb20_0::t_local_cmp_eq => 0
//@ run-call: Cb20_0::t_retdirect_cmp_eq => 0
//@ run-call: Cb20_0::t_param_cmp_eq => 0
//@ run-call: Cb20_0::t_mem_cmp_eq => 0
//@ run-call: Cb20_0::t_local_cmp_ne => 1
//@ run-call: Cb20_0::t_retdirect_cmp_ne => 1
//@ run-call: Cb20_0::t_param_cmp_ne => 1
//@ run-call: Cb20_0::t_mem_cmp_ne => 1
//@ run-call: Cb20_0::t_local_cmp_lt => 0
//@ run-call: Cb20_0::t_retdirect_cmp_lt => 0
//@ run-call: Cb20_0::t_param_cmp_lt => 0
//@ run-call: Cb20_0::t_mem_cmp_lt => 0
//@ run-call: Cb20_0::t_local_cmp_gt => 1
//@ run-call: Cb20_0::t_retdirect_cmp_gt => 1
//@ run-call: Cb20_0::t_param_cmp_gt => 1
//@ run-call: Cb20_0::t_mem_cmp_gt => 1
//@ run-call: Cb20_0::t_local_cmp_valid => 1
//@ run-call: Cb20_0::t_retdirect_cmp_valid => 1
//@ run-call: Cb20_0::t_param_cmp_valid => 1
//@ run-call: Cb20_0::t_mem_cmp_valid => 1

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

contract Cb1_0 {
    function _s_retdirect_cmp_eq() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmp_eq(bytes1 pv) internal view  returns (uint256) { return (pv == bytes1(0xcd)) ? 1 : 0; }
    function _s_retdirect_cmp_ne() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmp_ne(bytes1 pv) internal view  returns (uint256) { return (pv != bytes1(0xcd)) ? 1 : 0; }
    function _s_retdirect_cmp_lt() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmp_lt(bytes1 pv) internal view  returns (uint256) { return (pv < bytes1(0xcd)) ? 1 : 0; }
    function _s_retdirect_cmp_gt() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmp_gt(bytes1 pv) internal view  returns (uint256) { return (pv > bytes1(0xcd)) ? 1 : 0; }
    function _s_retdirect_cmp_valid() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmp_valid(bytes1 pv) internal view  returns (uint256) { return (pv == bytes1(0xab)) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return (rx == bytes1(0xcd)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == bytes1(0xcd)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return (ry == bytes1(0xcd)) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return (rx != bytes1(0xcd)) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (_s_retdirect_cmp_ne() != bytes1(0xcd)) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmp_ne(rx); }
    function t_mem_cmp_ne() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return (ry != bytes1(0xcd)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return (rx < bytes1(0xcd)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < bytes1(0xcd)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmp_lt(rx); }
    function t_mem_cmp_lt() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return (ry < bytes1(0xcd)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return (rx > bytes1(0xcd)) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (_s_retdirect_cmp_gt() > bytes1(0xcd)) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmp_gt(rx); }
    function t_mem_cmp_gt() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return (ry > bytes1(0xcd)) ? 1 : 0; }
    function t_local_cmp_valid() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return (rx == bytes1(0xab)) ? 1 : 0; }
    function t_retdirect_cmp_valid() external view  returns (uint256) { return (_s_retdirect_cmp_valid() == bytes1(0xab)) ? 1 : 0; }
    function t_param_cmp_valid() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmp_valid(rx); }
    function t_mem_cmp_valid() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return (ry == bytes1(0xab)) ? 1 : 0; }
}
contract Cb4_0 {
    function _s_retdirect_cmp_eq() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmp_eq() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmp_eq(bytes4 pv) internal view  returns (uint256) { return (pv == bytes4(0x11223344)) ? 1 : 0; }
    uint256 _res_modifier_cmp_eq;
    modifier _m_modifier_cmp_eq(bytes4 pv) { _res_modifier_cmp_eq = (pv == bytes4(0x11223344)) ? 1 : 0; _; }
    function _e_modifier_cmp_eq(bytes4 rd) internal _m_modifier_cmp_eq(rd)  returns (uint256) { return _res_modifier_cmp_eq; }
    function _w_ptr_cmp_eq() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmp_ne() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmp_ne() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmp_ne(bytes4 pv) internal view  returns (uint256) { return (pv != bytes4(0x11223344)) ? 1 : 0; }
    uint256 _res_modifier_cmp_ne;
    modifier _m_modifier_cmp_ne(bytes4 pv) { _res_modifier_cmp_ne = (pv != bytes4(0x11223344)) ? 1 : 0; _; }
    function _e_modifier_cmp_ne(bytes4 rd) internal _m_modifier_cmp_ne(rd)  returns (uint256) { return _res_modifier_cmp_ne; }
    function _w_ptr_cmp_ne() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmp_lt() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmp_lt() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmp_lt(bytes4 pv) internal view  returns (uint256) { return (pv < bytes4(0x11223344)) ? 1 : 0; }
    uint256 _res_modifier_cmp_lt;
    modifier _m_modifier_cmp_lt(bytes4 pv) { _res_modifier_cmp_lt = (pv < bytes4(0x11223344)) ? 1 : 0; _; }
    function _e_modifier_cmp_lt(bytes4 rd) internal _m_modifier_cmp_lt(rd)  returns (uint256) { return _res_modifier_cmp_lt; }
    function _w_ptr_cmp_lt() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmp_gt() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmp_gt() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmp_gt(bytes4 pv) internal view  returns (uint256) { return (pv > bytes4(0x11223344)) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return (rx == bytes4(0x11223344)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == bytes4(0x11223344)) ? 1 : 0; }
    function t_ret_cmp_eq() external view  returns (uint256) { bytes4 rx = _s_ret_cmp_eq(); return (rx == bytes4(0x11223344)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmp_eq(rx); }
    function t_copy_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_ternary_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_ifjoin_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_loopjoin_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_tuple_cmp_eq() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_modifier_cmp_eq() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmp_eq(rx); }
    function t_ptr_cmp_eq() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_eq; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_mem_cmp_eq() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return (ry == bytes4(0x11223344)) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return (rx != bytes4(0x11223344)) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (_s_retdirect_cmp_ne() != bytes4(0x11223344)) ? 1 : 0; }
    function t_ret_cmp_ne() external view  returns (uint256) { bytes4 rx = _s_ret_cmp_ne(); return (rx != bytes4(0x11223344)) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmp_ne(rx); }
    function t_copy_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_ternary_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_ifjoin_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_loopjoin_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_tuple_cmp_ne() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_modifier_cmp_ne() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmp_ne(rx); }
    function t_ptr_cmp_ne() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_ne; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_mem_cmp_ne() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return (ry != bytes4(0x11223344)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return (rx < bytes4(0x11223344)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < bytes4(0x11223344)) ? 1 : 0; }
    function t_ret_cmp_lt() external view  returns (uint256) { bytes4 rx = _s_ret_cmp_lt(); return (rx < bytes4(0x11223344)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmp_lt(rx); }
    function t_copy_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_ternary_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_ifjoin_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_loopjoin_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_tuple_cmp_lt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_modifier_cmp_lt() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmp_lt(rx); }
    function t_ptr_cmp_lt() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_lt; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_mem_cmp_lt() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return (ry < bytes4(0x11223344)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return (rx > bytes4(0x11223344)) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (_s_retdirect_cmp_gt() > bytes4(0x11223344)) ? 1 : 0; }
    function t_ret_cmp_gt() external view  returns (uint256) { bytes4 rx = _s_ret_cmp_gt(); return (rx > bytes4(0x11223344)) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmp_gt(rx); }
}
contract Cb4_1 {
    uint256 _res_modifier_cmp_gt;
    modifier _m_modifier_cmp_gt(bytes4 pv) { _res_modifier_cmp_gt = (pv > bytes4(0x11223344)) ? 1 : 0; _; }
    function _e_modifier_cmp_gt(bytes4 rd) internal _m_modifier_cmp_gt(rd)  returns (uint256) { return _res_modifier_cmp_gt; }
    function _w_ptr_cmp_gt() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmp_valid() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmp_valid() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmp_valid(bytes4 pv) internal view  returns (uint256) { return (pv == bytes4(0xdeadbeef)) ? 1 : 0; }
    uint256 _res_modifier_cmp_valid;
    modifier _m_modifier_cmp_valid(bytes4 pv) { _res_modifier_cmp_valid = (pv == bytes4(0xdeadbeef)) ? 1 : 0; _; }
    function _e_modifier_cmp_valid(bytes4 rd) internal _m_modifier_cmp_valid(rd)  returns (uint256) { return _res_modifier_cmp_valid; }
    function _w_ptr_cmp_valid() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_copy_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_ternary_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_ifjoin_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_loopjoin_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_tuple_cmp_gt() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_modifier_cmp_gt() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmp_gt(rx); }
    function t_ptr_cmp_gt() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_gt; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_mem_cmp_gt() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return (ry > bytes4(0x11223344)) ? 1 : 0; }
    function t_local_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return (rx == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_retdirect_cmp_valid() external view  returns (uint256) { return (_s_retdirect_cmp_valid() == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_ret_cmp_valid() external view  returns (uint256) { bytes4 rx = _s_ret_cmp_valid(); return (rx == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_param_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmp_valid(rx); }
    function t_copy_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_ternary_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_ifjoin_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_loopjoin_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_tuple_cmp_valid() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_modifier_cmp_valid() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmp_valid(rx); }
    function t_ptr_cmp_valid() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_valid; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
    function t_mem_cmp_valid() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return (ry == bytes4(0xdeadbeef)) ? 1 : 0; }
}
contract Cb20_0 {
    function _s_retdirect_cmp_eq() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmp_eq(bytes20 pv) internal view  returns (uint256) { return (pv == bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function _s_retdirect_cmp_ne() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmp_ne(bytes20 pv) internal view  returns (uint256) { return (pv != bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function _s_retdirect_cmp_lt() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmp_lt(bytes20 pv) internal view  returns (uint256) { return (pv < bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function _s_retdirect_cmp_gt() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmp_gt(bytes20 pv) internal view  returns (uint256) { return (pv > bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function _s_retdirect_cmp_valid() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmp_valid(bytes20 pv) internal view  returns (uint256) { return (pv == bytes20(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return (rx == bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (_s_retdirect_cmp_eq() == bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return (ry == bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return (rx != bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (_s_retdirect_cmp_ne() != bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmp_ne(rx); }
    function t_mem_cmp_ne() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return (ry != bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_local_cmp_lt() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return (rx < bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_retdirect_cmp_lt() external view  returns (uint256) { return (_s_retdirect_cmp_lt() < bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_param_cmp_lt() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmp_lt(rx); }
    function t_mem_cmp_lt() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return (ry < bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_local_cmp_gt() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return (rx > bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_retdirect_cmp_gt() external view  returns (uint256) { return (_s_retdirect_cmp_gt() > bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_param_cmp_gt() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmp_gt(rx); }
    function t_mem_cmp_gt() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return (ry > bytes20(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_local_cmp_valid() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return (rx == bytes20(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_retdirect_cmp_valid() external view  returns (uint256) { return (_s_retdirect_cmp_valid() == bytes20(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_param_cmp_valid() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmp_valid(rx); }
    function t_mem_cmp_valid() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return (ry == bytes20(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
}
