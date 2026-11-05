//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_to_uint_wide => 171
//@ run-call: Cb1_0::t_retdirect_to_uint_wide => 171
//@ run-call: Cb1_0::t_param_to_uint_wide => 171
//@ run-call: Cb1_0::t_mem_to_uint_wide => 171
//@ run-call: Cb4_0::t_local_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_retdirect_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_ret_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_param_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_copy_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_ternary_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_ifjoin_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_loopjoin_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_tuple_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_modifier_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_ptr_to_uint_wide => 3735928559
//@ run-call: Cb4_0::t_mem_to_uint_wide => 3735928559
//@ run-call: Cb20_0::t_local_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_uint_wide => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_uint_wide => 103929005307130220006098923584552504982110632080

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

contract Cb1_0 {
    function _s_retdirect_to_uint_wide() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_uint_wide(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv)) >> 0; }
    function t_local_to_uint_wide() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_wide())) >> 0; }
    function t_param_to_uint_wide() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry)) >> 0; }
}
contract Cb4_0 {
    function _s_retdirect_to_uint_wide() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_uint_wide() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_uint_wide(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv)) >> 0; }
    uint256 _res_modifier_to_uint_wide;
    modifier _m_modifier_to_uint_wide(bytes4 pv) { _res_modifier_to_uint_wide = uint256(uint32(pv)) >> 0; _; }
    function _e_modifier_to_uint_wide(bytes4 rd) internal _m_modifier_to_uint_wide(rd)  returns (uint256) { return _res_modifier_to_uint_wide; }
    function _w_ptr_to_uint_wide() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_local_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view  returns (uint256) { return uint256(uint32(_s_retdirect_to_uint_wide())) >> 0; }
    function t_ret_to_uint_wide() external view  returns (uint256) { bytes4 rx = _s_ret_to_uint_wide(); return uint256(uint32(rx)) >> 0; }
    function t_param_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_uint_wide(rx); }
    function t_copy_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry)) >> 0; }
    function t_ternary_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry)) >> 0; }
    function t_ifjoin_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)) >> 0; }
    function t_loopjoin_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)) >> 0; }
    function t_tuple_to_uint_wide() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)) >> 0; }
    function t_modifier_to_uint_wide() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_uint_wide(rx); }
    function t_ptr_to_uint_wide() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint_wide; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry)) >> 0; }
    function t_mem_to_uint_wide() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry)) >> 0; }
}
contract Cb20_0 {
    function _s_retdirect_to_uint_wide() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_uint_wide(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv)) >> 0; }
    function t_local_to_uint_wide() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx)) >> 0; }
    function t_retdirect_to_uint_wide() external view  returns (uint256) { return uint256(uint160(_s_retdirect_to_uint_wide())) >> 0; }
    function t_param_to_uint_wide() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry)) >> 0; }
}
