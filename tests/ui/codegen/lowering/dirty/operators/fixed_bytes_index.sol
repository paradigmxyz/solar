//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_index0 => 171
//@ run-call: Cb1_0::t_retdirect_index0 => 171
//@ run-call: Cb1_0::t_param_index0 => 171
//@ run-call: Cb1_0::t_mem_index0 => 171
//@ run-call: Cb1_0::t_local_index_last => 171
//@ run-call: Cb1_0::t_retdirect_index_last => 171
//@ run-call: Cb1_0::t_param_index_last => 171
//@ run-call: Cb1_0::t_mem_index_last => 171
//@ run-call: Cb4_0::t_local_index0 => 222
//@ run-call: Cb4_0::t_retdirect_index0 => 222
//@ run-call: Cb4_0::t_ret_index0 => 222
//@ run-call: Cb4_0::t_param_index0 => 222
//@ run-call: Cb4_0::t_copy_index0 => 222
//@ run-call: Cb4_0::t_ternary_index0 => 222
//@ run-call: Cb4_0::t_ifjoin_index0 => 222
//@ run-call: Cb4_0::t_loopjoin_index0 => 222
//@ run-call: Cb4_0::t_tuple_index0 => 222
//@ run-call: Cb4_0::t_modifier_index0 => 222
//@ run-call: Cb4_0::t_ptr_index0 => 222
//@ run-call: Cb4_0::t_mem_index0 => 222
//@ run-call: Cb4_0::t_local_index_last => 239
//@ run-call: Cb4_0::t_retdirect_index_last => 239
//@ run-call: Cb4_0::t_ret_index_last => 239
//@ run-call: Cb4_0::t_param_index_last => 239
//@ run-call: Cb4_0::t_copy_index_last => 239
//@ run-call: Cb4_0::t_ternary_index_last => 239
//@ run-call: Cb4_0::t_ifjoin_index_last => 239
//@ run-call: Cb4_0::t_loopjoin_index_last => 239
//@ run-call: Cb4_0::t_tuple_index_last => 239
//@ run-call: Cb4_0::t_modifier_index_last => 239
//@ run-call: Cb4_0::t_ptr_index_last => 239
//@ run-call: Cb4_0::t_mem_index_last => 239
//@ run-call: Cb20_0::t_local_index0 => 18
//@ run-call: Cb20_0::t_retdirect_index0 => 18
//@ run-call: Cb20_0::t_param_index0 => 18
//@ run-call: Cb20_0::t_mem_index0 => 18
//@ run-call: Cb20_0::t_local_index_last => 144
//@ run-call: Cb20_0::t_retdirect_index_last => 144
//@ run-call: Cb20_0::t_param_index_last => 144
//@ run-call: Cb20_0::t_mem_index_last => 144

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
    function _s_retdirect_index0() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_index0(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv[0])); }
    function _s_retdirect_index_last() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_index_last(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv[0])); }
    function t_local_index0() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx[0])); }
    function t_retdirect_index0() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index0()[0])); }
    function t_param_index0() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_index0(rx); }
    function t_mem_index0() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry[0])); }
    function t_local_index_last() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx[0])); }
    function t_retdirect_index_last() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index_last()[0])); }
    function t_param_index_last() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_index_last(rx); }
    function t_mem_index_last() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry[0])); }
}
contract Cb4_0 {
    function _s_retdirect_index0() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_index0() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_index0(bytes4 pv) internal view  returns (uint256) { return uint256(uint8(pv[0])); }
    uint256 _res_modifier_index0;
    modifier _m_modifier_index0(bytes4 pv) { _res_modifier_index0 = uint256(uint8(pv[0])); _; }
    function _e_modifier_index0(bytes4 rd) internal _m_modifier_index0(rd)  returns (uint256) { return _res_modifier_index0; }
    function _w_ptr_index0() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_index_last() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_index_last() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_index_last(bytes4 pv) internal view  returns (uint256) { return uint256(uint8(pv[3])); }
    uint256 _res_modifier_index_last;
    modifier _m_modifier_index_last(bytes4 pv) { _res_modifier_index_last = uint256(uint8(pv[3])); _; }
    function _e_modifier_index_last(bytes4 rd) internal _m_modifier_index_last(rd)  returns (uint256) { return _res_modifier_index_last; }
    function _w_ptr_index_last() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_local_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint8(rx[0])); }
    function t_retdirect_index0() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index0()[0])); }
    function t_ret_index0() external view  returns (uint256) { bytes4 rx = _s_ret_index0(); return uint256(uint8(rx[0])); }
    function t_param_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_index0(rx); }
    function t_copy_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint8(ry[0])); }
    function t_ternary_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint8(ry[0])); }
    function t_ifjoin_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry[0])); }
    function t_loopjoin_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry[0])); }
    function t_tuple_index0() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry[0])); }
    function t_modifier_index0() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_index0(rx); }
    function t_ptr_index0() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_index0; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint8(ry[0])); }
    function t_mem_index0() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint8(ry[0])); }
    function t_local_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint8(rx[3])); }
    function t_retdirect_index_last() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index_last()[3])); }
    function t_ret_index_last() external view  returns (uint256) { bytes4 rx = _s_ret_index_last(); return uint256(uint8(rx[3])); }
    function t_param_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_index_last(rx); }
    function t_copy_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint8(ry[3])); }
    function t_ternary_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint8(ry[3])); }
    function t_ifjoin_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry[3])); }
    function t_loopjoin_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry[3])); }
    function t_tuple_index_last() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry[3])); }
    function t_modifier_index_last() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_index_last(rx); }
    function t_ptr_index_last() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_index_last; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint8(ry[3])); }
    function t_mem_index_last() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint8(ry[3])); }
}
contract Cb20_0 {
    function _s_retdirect_index0() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_index0(bytes20 pv) internal view  returns (uint256) { return uint256(uint8(pv[0])); }
    function _s_retdirect_index_last() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_index_last(bytes20 pv) internal view  returns (uint256) { return uint256(uint8(pv[19])); }
    function t_local_index0() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint8(rx[0])); }
    function t_retdirect_index0() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index0()[0])); }
    function t_param_index0() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_index0(rx); }
    function t_mem_index0() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint8(ry[0])); }
    function t_local_index_last() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint8(rx[19])); }
    function t_retdirect_index_last() external view  returns (uint256) { return uint256(uint8(_s_retdirect_index_last()[19])); }
    function t_param_index_last() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_index_last(rx); }
    function t_mem_index_last() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint8(ry[19])); }
}
