//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_to_uint16 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint16 => 27
//@ run-call: Cu8_0::t_ret_to_uint16 => 27
//@ run-call: Cu8_0::t_param_to_uint16 => 27
//@ run-call: Cu8_0::t_copy_to_uint16 => 27
//@ run-call: Cu8_0::t_ternary_to_uint16 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint16 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint16 => 27
//@ run-call: Cu8_0::t_tuple_to_uint16 => 27
//@ run-call: Cu8_0::t_modifier_to_uint16 => 27
//@ run-call: Cu8_0::t_ptr_to_uint16 => 27
//@ run-call: Cu8_0::t_mem_to_uint16 => 27
//@ run-call: Cu8_0::t_local_to_uint64 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint64 => 27
//@ run-call: Cu8_0::t_ret_to_uint64 => 27
//@ run-call: Cu8_0::t_param_to_uint64 => 27
//@ run-call: Cu8_0::t_copy_to_uint64 => 27
//@ run-call: Cu8_0::t_ternary_to_uint64 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint64 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint64 => 27
//@ run-call: Cu8_0::t_tuple_to_uint64 => 27
//@ run-call: Cu8_0::t_modifier_to_uint64 => 27
//@ run-call: Cu8_0::t_ptr_to_uint64 => 27
//@ run-call: Cu8_0::t_mem_to_uint64 => 27
//@ run-call: Cu8_0::t_local_to_uint128 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint128 => 27
//@ run-call: Cu8_0::t_ret_to_uint128 => 27
//@ run-call: Cu8_0::t_param_to_uint128 => 27
//@ run-call: Cu8_0::t_copy_to_uint128 => 27
//@ run-call: Cu8_0::t_ternary_to_uint128 => 27
//@ run-call: Cu8_0::t_ifjoin_to_uint128 => 27
//@ run-call: Cu8_0::t_loopjoin_to_uint128 => 27
//@ run-call: Cu8_0::t_tuple_to_uint128 => 27
//@ run-call: Cu8_0::t_modifier_to_uint128 => 27
//@ run-call: Cu8_0::t_ptr_to_uint128 => 27
//@ run-call: Cu8_0::t_mem_to_uint128 => 27
//@ run-call: Cu8_0::t_local_to_uint256 => 27
//@ run-call: Cu8_0::t_retdirect_to_uint256 => 27
//@ run-call: Cu8_0::t_ret_to_uint256 => 27
//@ run-call: Cu8_0::t_param_to_uint256 => 27
//@ run-call: Cu8_1::t_copy_to_uint256 => 27
//@ run-call: Cu8_1::t_ternary_to_uint256 => 27
//@ run-call: Cu8_1::t_ifjoin_to_uint256 => 27
//@ run-call: Cu8_1::t_loopjoin_to_uint256 => 27
//@ run-call: Cu8_1::t_tuple_to_uint256 => 27
//@ run-call: Cu8_1::t_modifier_to_uint256 => 27
//@ run-call: Cu8_1::t_ptr_to_uint256 => 27
//@ run-call: Cu8_1::t_mem_to_uint256 => 27
//@ run-call: Cu8_1::t_local_to_int_same => 27
//@ run-call: Cu8_1::t_retdirect_to_int_same => 27
//@ run-call: Cu8_1::t_ret_to_int_same => 27
//@ run-call: Cu8_1::t_param_to_int_same => 27
//@ run-call: Cu8_1::t_copy_to_int_same => 27
//@ run-call: Cu8_1::t_ternary_to_int_same => 27
//@ run-call: Cu8_1::t_ifjoin_to_int_same => 27
//@ run-call: Cu8_1::t_loopjoin_to_int_same => 27
//@ run-call: Cu8_1::t_tuple_to_int_same => 27
//@ run-call: Cu8_1::t_modifier_to_int_same => 27
//@ run-call: Cu8_1::t_ptr_to_int_same => 27
//@ run-call: Cu8_1::t_mem_to_int_same => 27
//@ run-call: Cu8_1::t_local_to_int_wide => 27
//@ run-call: Cu8_1::t_retdirect_to_int_wide => 27
//@ run-call: Cu8_1::t_ret_to_int_wide => 27
//@ run-call: Cu8_1::t_param_to_int_wide => 27
//@ run-call: Cu8_1::t_copy_to_int_wide => 27
//@ run-call: Cu8_1::t_ternary_to_int_wide => 27
//@ run-call: Cu8_1::t_ifjoin_to_int_wide => 27
//@ run-call: Cu8_1::t_loopjoin_to_int_wide => 27
//@ run-call: Cu8_1::t_tuple_to_int_wide => 27
//@ run-call: Cu8_1::t_modifier_to_int_wide => 27
//@ run-call: Cu8_1::t_ptr_to_int_wide => 27
//@ run-call: Cu8_1::t_mem_to_int_wide => 27
//@ run-call: Cu8_1::t_local_implicit_widen => 27
//@ run-call: Cu8_1::t_retdirect_implicit_widen => 27
//@ run-call: Cu8_1::t_ret_implicit_widen => 27
//@ run-call: Cu8_1::t_param_implicit_widen => 27
//@ run-call: Cu8_1::t_copy_implicit_widen => 27
//@ run-call: Cu8_1::t_ternary_implicit_widen => 27
//@ run-call: Cu8_1::t_ifjoin_implicit_widen => 27
//@ run-call: Cu8_1::t_loopjoin_implicit_widen => 27
//@ run-call: Cu8_2::t_tuple_implicit_widen => 27
//@ run-call: Cu8_2::t_modifier_implicit_widen => 27
//@ run-call: Cu8_2::t_ptr_implicit_widen => 27
//@ run-call: Cu8_2::t_mem_implicit_widen => 27
//@ run-call: Cu8_2::t_local_narrow_uint8 => 27
//@ run-call: Cu8_2::t_retdirect_narrow_uint8 => 27
//@ run-call: Cu8_2::t_ret_narrow_uint8 => 27
//@ run-call: Cu8_2::t_param_narrow_uint8 => 27
//@ run-call: Cu8_2::t_copy_narrow_uint8 => 27
//@ run-call: Cu8_2::t_ternary_narrow_uint8 => 27
//@ run-call: Cu8_2::t_ifjoin_narrow_uint8 => 27
//@ run-call: Cu8_2::t_loopjoin_narrow_uint8 => 27
//@ run-call: Cu8_2::t_tuple_narrow_uint8 => 27
//@ run-call: Cu8_2::t_modifier_narrow_uint8 => 27
//@ run-call: Cu8_2::t_ptr_narrow_uint8 => 27
//@ run-call: Cu8_2::t_mem_narrow_uint8 => 27
//@ run-call: Cu8_2::t_local_narrow_uint32 => 27
//@ run-call: Cu8_2::t_retdirect_narrow_uint32 => 27
//@ run-call: Cu8_2::t_ret_narrow_uint32 => 27
//@ run-call: Cu8_2::t_param_narrow_uint32 => 27
//@ run-call: Cu8_2::t_copy_narrow_uint32 => 27
//@ run-call: Cu8_2::t_ternary_narrow_uint32 => 27
//@ run-call: Cu8_2::t_ifjoin_narrow_uint32 => 27
//@ run-call: Cu8_2::t_loopjoin_narrow_uint32 => 27
//@ run-call: Cu8_2::t_tuple_narrow_uint32 => 27
//@ run-call: Cu8_2::t_modifier_narrow_uint32 => 27
//@ run-call: Cu8_2::t_ptr_narrow_uint32 => 27
//@ run-call: Cu8_2::t_mem_narrow_uint32 => 27
//@ run-call: Cu64_0::t_local_to_uint8 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint8 => 27
//@ run-call: Cu64_0::t_param_to_uint8 => 27
//@ run-call: Cu64_0::t_mem_to_uint8 => 27
//@ run-call: Cu64_0::t_local_to_uint16 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint16 => 27
//@ run-call: Cu64_0::t_param_to_uint16 => 27
//@ run-call: Cu64_0::t_mem_to_uint16 => 27
//@ run-call: Cu64_0::t_local_to_uint128 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint128 => 27
//@ run-call: Cu64_0::t_param_to_uint128 => 27
//@ run-call: Cu64_0::t_mem_to_uint128 => 27
//@ run-call: Cu64_0::t_local_to_uint256 => 27
//@ run-call: Cu64_0::t_retdirect_to_uint256 => 27
//@ run-call: Cu64_0::t_param_to_uint256 => 27
//@ run-call: Cu64_0::t_mem_to_uint256 => 27
//@ run-call: Cu64_0::t_local_to_int_same => 27
//@ run-call: Cu64_0::t_retdirect_to_int_same => 27
//@ run-call: Cu64_0::t_param_to_int_same => 27
//@ run-call: Cu64_0::t_mem_to_int_same => 27
//@ run-call: Cu64_0::t_local_to_int_wide => 27
//@ run-call: Cu64_0::t_retdirect_to_int_wide => 27
//@ run-call: Cu64_0::t_param_to_int_wide => 27
//@ run-call: Cu64_0::t_mem_to_int_wide => 27
//@ run-call: Cu64_0::t_local_implicit_widen => 27
//@ run-call: Cu64_0::t_retdirect_implicit_widen => 27
//@ run-call: Cu64_0::t_param_implicit_widen => 27
//@ run-call: Cu64_0::t_mem_implicit_widen => 27
//@ run-call: Cu64_0::t_local_narrow_uint8 => 27
//@ run-call: Cu64_0::t_retdirect_narrow_uint8 => 27
//@ run-call: Cu64_0::t_param_narrow_uint8 => 27
//@ run-call: Cu64_0::t_mem_narrow_uint8 => 27
//@ run-call: Cu64_0::t_local_narrow_uint32 => 27
//@ run-call: Cu64_0::t_retdirect_narrow_uint32 => 27
//@ run-call: Cu64_0::t_param_narrow_uint32 => 27
//@ run-call: Cu64_0::t_mem_narrow_uint32 => 27
//@ run-call: Ci8_0::t_local_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_uint_same => 253
//@ run-call: Ci8_0::t_retdirect_to_uint_same => 253
//@ run-call: Ci8_0::t_param_to_uint_same => 253
//@ run-call: Ci8_0::t_mem_to_uint_same => 253
//@ run-call: Ci8_0::t_local_to_uint_wide => 65533
//@ run-call: Ci8_0::t_retdirect_to_uint_wide => 65533
//@ run-call: Ci8_0::t_param_to_uint_wide => 65533
//@ run-call: Ci8_0::t_mem_to_uint_wide => 65533
//@ run-call: Ci8_0::t_local_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int16 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int64 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int128 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_int256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_uint_same => 16777213
//@ run-call: Ci24_0::t_retdirect_to_uint_same => 16777213
//@ run-call: Ci24_0::t_param_to_uint_same => 16777213
//@ run-call: Ci24_0::t_mem_to_uint_same => 16777213
//@ run-call: Ci24_0::t_local_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_retdirect_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_param_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_mem_to_uint_wide => 281474976710653
//@ run-call: Ci24_0::t_local_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_uint256 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_to_uint256_wide => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_implicit_widen => 115792089237316195423570985008687907853269984665640564039457584007913129639933

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
    function _s_retdirect_to_uint16() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint16() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint16(uint8 pv) internal view  returns (uint256) { return uint256(uint16(pv)); }
    uint256 _res_modifier_to_uint16;
    modifier _m_modifier_to_uint16(uint8 pv) { _res_modifier_to_uint16 = uint256(uint16(pv)); _; }
    function _e_modifier_to_uint16(uint8 rd) internal _m_modifier_to_uint16(rd)  returns (uint256) { return _res_modifier_to_uint16; }
    function _w_ptr_to_uint16() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint64() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint64() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint64(uint8 pv) internal view  returns (uint256) { return uint256(uint64(pv)); }
    uint256 _res_modifier_to_uint64;
    modifier _m_modifier_to_uint64(uint8 pv) { _res_modifier_to_uint64 = uint256(uint64(pv)); _; }
    function _e_modifier_to_uint64(uint8 rd) internal _m_modifier_to_uint64(rd)  returns (uint256) { return _res_modifier_to_uint64; }
    function _w_ptr_to_uint64() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint128() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint128() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint128(uint8 pv) internal view  returns (uint256) { return uint256(uint128(pv)); }
    uint256 _res_modifier_to_uint128;
    modifier _m_modifier_to_uint128(uint8 pv) { _res_modifier_to_uint128 = uint256(uint128(pv)); _; }
    function _e_modifier_to_uint128(uint8 rd) internal _m_modifier_to_uint128(rd)  returns (uint256) { return _res_modifier_to_uint128; }
    function _w_ptr_to_uint128() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_uint256() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_uint256() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_uint256(uint8 pv) internal view  returns (uint256) { return uint256(uint256(pv)); }
    function t_local_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint16(rx)); }
    function t_retdirect_to_uint16() external view  returns (uint256) { return uint256(uint16(_s_retdirect_to_uint16())); }
    function t_ret_to_uint16() external view  returns (uint256) { uint8 rx = _s_ret_to_uint16(); return uint256(uint16(rx)); }
    function t_param_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint16(rx); }
    function t_copy_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint16(ry)); }
    function t_ternary_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint16(ry)); }
    function t_ifjoin_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(ry)); }
    function t_loopjoin_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(ry)); }
    function t_tuple_to_uint16() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(ry)); }
    function t_modifier_to_uint16() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint16(rx); }
    function t_ptr_to_uint16() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint16; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint16(ry)); }
    function t_mem_to_uint16() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint16(ry)); }
    function t_local_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint64(rx)); }
    function t_retdirect_to_uint64() external view  returns (uint256) { return uint256(uint64(_s_retdirect_to_uint64())); }
    function t_ret_to_uint64() external view  returns (uint256) { uint8 rx = _s_ret_to_uint64(); return uint256(uint64(rx)); }
    function t_param_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint64(rx); }
    function t_copy_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint64(ry)); }
    function t_ternary_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint64(ry)); }
    function t_ifjoin_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(ry)); }
    function t_loopjoin_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(ry)); }
    function t_tuple_to_uint64() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(ry)); }
    function t_modifier_to_uint64() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint64(rx); }
    function t_ptr_to_uint64() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint64; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint64(ry)); }
    function t_mem_to_uint64() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint64(ry)); }
    function t_local_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint128(rx)); }
    function t_retdirect_to_uint128() external view  returns (uint256) { return uint256(uint128(_s_retdirect_to_uint128())); }
    function t_ret_to_uint128() external view  returns (uint256) { uint8 rx = _s_ret_to_uint128(); return uint256(uint128(rx)); }
    function t_param_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint128(rx); }
    function t_copy_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint128(ry)); }
    function t_ternary_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint128(ry)); }
    function t_ifjoin_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint128(ry)); }
    function t_loopjoin_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint128(ry)); }
    function t_tuple_to_uint128() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint128(ry)); }
    function t_modifier_to_uint128() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint128(rx); }
    function t_ptr_to_uint128() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint128; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint128(ry)); }
    function t_mem_to_uint128() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint128(ry)); }
    function t_local_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint256(rx)); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint256(_s_retdirect_to_uint256())); }
    function t_ret_to_uint256() external view  returns (uint256) { uint8 rx = _s_ret_to_uint256(); return uint256(uint256(rx)); }
    function t_param_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_uint256(rx); }
}
contract Cu8_1 {
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(uint8 pv) { _res_modifier_to_uint256 = uint256(uint256(pv)); _; }
    function _e_modifier_to_uint256(uint8 rd) internal _m_modifier_to_uint256(rd)  returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_int_same() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_int_same() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_int_same(uint8 pv) internal view  returns (uint256) { return uint256(uint8(int8(pv))); }
    uint256 _res_modifier_to_int_same;
    modifier _m_modifier_to_int_same(uint8 pv) { _res_modifier_to_int_same = uint256(uint8(int8(pv))); _; }
    function _e_modifier_to_int_same(uint8 rd) internal _m_modifier_to_int_same(rd)  returns (uint256) { return _res_modifier_to_int_same; }
    function _w_ptr_to_int_same() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_to_int_wide() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_int_wide() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_int_wide(uint8 pv) internal view  returns (uint256) { return uint256(int256(int16(uint16(pv)))); }
    uint256 _res_modifier_to_int_wide;
    modifier _m_modifier_to_int_wide(uint8 pv) { _res_modifier_to_int_wide = uint256(int256(int16(uint16(pv)))); _; }
    function _e_modifier_to_int_wide(uint8 rd) internal _m_modifier_to_int_wide(rd)  returns (uint256) { return _res_modifier_to_int_wide; }
    function _w_ptr_to_int_wide() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_implicit_widen() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_implicit_widen() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_implicit_widen(uint8 pv) internal view  returns (uint256) { return uint256(pv) + 0; }
    function t_copy_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint256(ry)); }
    function t_ternary_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint256(ry)); }
    function t_ifjoin_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(ry)); }
    function t_loopjoin_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(ry)); }
    function t_tuple_to_uint256() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(ry)); }
    function t_modifier_to_uint256() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint256(ry)); }
    function t_mem_to_uint256() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint256(ry)); }
    function t_local_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(int8(rx))); }
    function t_retdirect_to_int_same() external view  returns (uint256) { return uint256(uint8(int8(_s_retdirect_to_int_same()))); }
    function t_ret_to_int_same() external view  returns (uint256) { uint8 rx = _s_ret_to_int_same(); return uint256(uint8(int8(rx))); }
    function t_param_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_int_same(rx); }
    function t_copy_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(int8(ry))); }
    function t_ternary_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(int8(ry))); }
    function t_ifjoin_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(int8(ry))); }
    function t_loopjoin_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(int8(ry))); }
    function t_tuple_to_int_same() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(int8(ry))); }
    function t_modifier_to_int_same() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_int_same(rx); }
    function t_ptr_to_int_same() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_int_same; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(int8(ry))); }
    function t_mem_to_int_same() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(int8(ry))); }
    function t_local_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(int256(int16(uint16(rx)))); }
    function t_retdirect_to_int_wide() external view  returns (uint256) { return uint256(int256(int16(uint16(_s_retdirect_to_int_wide())))); }
    function t_ret_to_int_wide() external view  returns (uint256) { uint8 rx = _s_ret_to_int_wide(); return uint256(int256(int16(uint16(rx)))); }
    function t_param_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_int_wide(rx); }
    function t_copy_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(int256(int16(uint16(ry)))); }
    function t_ternary_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(int256(int16(uint16(ry)))); }
    function t_ifjoin_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(int256(int16(uint16(ry)))); }
    function t_loopjoin_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(int256(int16(uint16(ry)))); }
    function t_tuple_to_int_wide() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(int256(int16(uint16(ry)))); }
    function t_modifier_to_int_wide() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_int_wide(rx); }
    function t_ptr_to_int_wide() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_int_wide; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(int256(int16(uint16(ry)))); }
    function t_mem_to_int_wide() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(int256(int16(uint16(ry)))); }
    function t_local_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx) + 0; }
    function t_retdirect_implicit_widen() external view  returns (uint256) { return uint256(_s_retdirect_implicit_widen()) + 0; }
    function t_ret_implicit_widen() external view  returns (uint256) { uint8 rx = _s_ret_implicit_widen(); return uint256(rx) + 0; }
    function t_param_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_implicit_widen(rx); }
    function t_copy_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry) + 0; }
    function t_ternary_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry) + 0; }
    function t_ifjoin_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry) + 0; }
    function t_loopjoin_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry) + 0; }
}
contract Cu8_2 {
    uint256 _res_modifier_implicit_widen;
    modifier _m_modifier_implicit_widen(uint8 pv) { _res_modifier_implicit_widen = uint256(pv) + 0; _; }
    function _e_modifier_implicit_widen(uint8 rd) internal _m_modifier_implicit_widen(rd)  returns (uint256) { return _res_modifier_implicit_widen; }
    function _w_ptr_implicit_widen() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_narrow_uint8() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_narrow_uint8() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_narrow_uint8(uint8 pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    uint256 _res_modifier_narrow_uint8;
    modifier _m_modifier_narrow_uint8(uint8 pv) { _res_modifier_narrow_uint8 = uint256(uint8(pv)); _; }
    function _e_modifier_narrow_uint8(uint8 rd) internal _m_modifier_narrow_uint8(rd)  returns (uint256) { return _res_modifier_narrow_uint8; }
    function _w_ptr_narrow_uint8() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_narrow_uint32() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_narrow_uint32() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_narrow_uint32(uint8 pv) internal view  returns (uint256) { return uint256(uint32(pv)); }
    uint256 _res_modifier_narrow_uint32;
    modifier _m_modifier_narrow_uint32(uint8 pv) { _res_modifier_narrow_uint32 = uint256(uint32(pv)); _; }
    function _e_modifier_narrow_uint32(uint8 rd) internal _m_modifier_narrow_uint32(rd)  returns (uint256) { return _res_modifier_narrow_uint32; }
    function _w_ptr_narrow_uint32() internal pure returns (uint256) { return 0xabcd1b; }
    function t_tuple_implicit_widen() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry) + 0; }
    function t_modifier_implicit_widen() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_implicit_widen(rx); }
    function t_ptr_implicit_widen() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_implicit_widen; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry) + 0; }
    function t_mem_implicit_widen() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry) + 0; }
    function t_local_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(rx)); }
    function t_retdirect_narrow_uint8() external view  returns (uint256) { return uint256(uint8(_s_retdirect_narrow_uint8())); }
    function t_ret_narrow_uint8() external view  returns (uint256) { uint8 rx = _s_ret_narrow_uint8(); return uint256(uint8(rx)); }
    function t_param_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_narrow_uint8(rx); }
    function t_copy_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(ry)); }
    function t_ternary_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(ry)); }
    function t_ifjoin_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry)); }
    function t_loopjoin_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry)); }
    function t_tuple_narrow_uint8() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry)); }
    function t_modifier_narrow_uint8() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_narrow_uint8(rx); }
    function t_ptr_narrow_uint8() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_narrow_uint8; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(ry)); }
    function t_mem_narrow_uint8() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint32(rx)); }
    function t_retdirect_narrow_uint32() external view  returns (uint256) { return uint256(uint32(_s_retdirect_narrow_uint32())); }
    function t_ret_narrow_uint32() external view  returns (uint256) { uint8 rx = _s_ret_narrow_uint32(); return uint256(uint32(rx)); }
    function t_param_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_narrow_uint32(rx); }
    function t_copy_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint32(ry)); }
    function t_ternary_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint32(ry)); }
    function t_ifjoin_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)); }
    function t_loopjoin_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)); }
    function t_tuple_narrow_uint32() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)); }
    function t_modifier_narrow_uint32() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_narrow_uint32(rx); }
    function t_ptr_narrow_uint32() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_narrow_uint32; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint32(ry)); }
    function t_mem_narrow_uint32() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint32(ry)); }
}
contract Cu64_0 {
    function _s_retdirect_to_uint8() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint8(uint64 pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_uint16() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint16(uint64 pv) internal view  returns (uint256) { return uint256(uint16(pv)); }
    function _s_retdirect_to_uint128() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint128(uint64 pv) internal view  returns (uint256) { return uint256(uint128(pv)); }
    function _s_retdirect_to_uint256() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_uint256(uint64 pv) internal view  returns (uint256) { return uint256(uint256(pv)); }
    function _s_retdirect_to_int_same() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_int_same(uint64 pv) internal view  returns (uint256) { return uint256(uint64(int64(pv))); }
    function _s_retdirect_to_int_wide() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_int_wide(uint64 pv) internal view  returns (uint256) { return uint256(int256(int128(uint128(pv)))); }
    function _s_retdirect_implicit_widen() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_implicit_widen(uint64 pv) internal view  returns (uint256) { return uint256(pv) + 0; }
    function _s_retdirect_narrow_uint8() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_narrow_uint8(uint64 pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_narrow_uint32() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_narrow_uint32(uint64 pv) internal view  returns (uint256) { return uint256(uint32(pv)); }
    function t_local_to_uint8() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(rx)); }
    function t_retdirect_to_uint8() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint8())); }
    function t_param_to_uint8() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint16() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint16(rx)); }
    function t_retdirect_to_uint16() external view  returns (uint256) { return uint256(uint16(_s_retdirect_to_uint16())); }
    function t_param_to_uint16() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint16(rx); }
    function t_mem_to_uint16() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint16(ry)); }
    function t_local_to_uint128() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint128(rx)); }
    function t_retdirect_to_uint128() external view  returns (uint256) { return uint256(uint128(_s_retdirect_to_uint128())); }
    function t_param_to_uint128() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint128(rx); }
    function t_mem_to_uint128() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint128(ry)); }
    function t_local_to_uint256() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint256(rx)); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint256(ry)); }
    function t_local_to_int_same() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(int64(rx))); }
    function t_retdirect_to_int_same() external view  returns (uint256) { return uint256(uint64(int64(_s_retdirect_to_int_same()))); }
    function t_param_to_int_same() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_int_same(rx); }
    function t_mem_to_int_same() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(int64(ry))); }
    function t_local_to_int_wide() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(int256(int128(uint128(rx)))); }
    function t_retdirect_to_int_wide() external view  returns (uint256) { return uint256(int256(int128(uint128(_s_retdirect_to_int_wide())))); }
    function t_param_to_int_wide() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_int_wide(rx); }
    function t_mem_to_int_wide() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(int256(int128(uint128(ry)))); }
    function t_local_implicit_widen() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx) + 0; }
    function t_retdirect_implicit_widen() external view  returns (uint256) { return uint256(_s_retdirect_implicit_widen()) + 0; }
    function t_param_implicit_widen() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry) + 0; }
    function t_local_narrow_uint8() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(rx)); }
    function t_retdirect_narrow_uint8() external view  returns (uint256) { return uint256(uint8(_s_retdirect_narrow_uint8())); }
    function t_param_narrow_uint8() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_narrow_uint8(rx); }
    function t_mem_narrow_uint8() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_narrow_uint32() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint32(rx)); }
    function t_retdirect_narrow_uint32() external view  returns (uint256) { return uint256(uint32(_s_retdirect_narrow_uint32())); }
    function t_param_narrow_uint32() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_narrow_uint32(rx); }
    function t_mem_narrow_uint32() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint32(ry)); }
}
contract Ci8_0 {
    function _s_retdirect_to_int16() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int16(int8 pv) internal view  returns (uint256) { return uint256(int256(int16(pv))); }
    function _s_retdirect_to_int64() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int64(int8 pv) internal view  returns (uint256) { return uint256(int256(int64(pv))); }
    function _s_retdirect_to_int128() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int128(int8 pv) internal view  returns (uint256) { return uint256(int256(int128(pv))); }
    function _s_retdirect_to_int256() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_int256(int8 pv) internal view  returns (uint256) { return uint256(int256(int256(pv))); }
    function _s_retdirect_to_uint_same() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint_same(int8 pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint_wide(int8 pv) internal view  returns (uint256) { return uint256(uint16(int16(pv))); }
    function _s_retdirect_to_uint256() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint256(int8 pv) internal view  returns (uint256) { return uint256(int256(pv)); }
    function _s_retdirect_to_uint256_wide() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_to_uint256_wide(int8 pv) internal view  returns (uint256) { return uint256(uint256(int256(pv))); }
    function _s_retdirect_implicit_widen() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_implicit_widen(int8 pv) internal view  returns (uint256) { return uint256(int256(pv) + 0); }
    function t_local_to_int16() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int16(rx))); }
    function t_retdirect_to_int16() external view  returns (uint256) { return uint256(int256(int16(_s_retdirect_to_int16()))); }
    function t_param_to_int16() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int16(rx); }
    function t_mem_to_int16() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int16(ry))); }
    function t_local_to_int64() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int64(rx))); }
    function t_retdirect_to_int64() external view  returns (uint256) { return uint256(int256(int64(_s_retdirect_to_int64()))); }
    function t_param_to_int64() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int64(rx); }
    function t_mem_to_int64() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int64(ry))); }
    function t_local_to_int128() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int128(rx))); }
    function t_retdirect_to_int128() external view  returns (uint256) { return uint256(int256(int128(_s_retdirect_to_int128()))); }
    function t_param_to_int128() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int128(rx); }
    function t_mem_to_int128() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int128(ry))); }
    function t_local_to_int256() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int256(rx))); }
    function t_retdirect_to_int256() external view  returns (uint256) { return uint256(int256(int256(_s_retdirect_to_int256()))); }
    function t_param_to_int256() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_int256(rx); }
    function t_mem_to_int256() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int256(ry))); }
    function t_local_to_uint_same() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint8(rx)); }
    function t_retdirect_to_uint_same() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint_same())); }
    function t_param_to_uint_same() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint_same(rx); }
    function t_mem_to_uint_same() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_uint_wide() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint16(int16(rx))); }
    function t_retdirect_to_uint_wide() external view  returns (uint256) { return uint256(uint16(int16(_s_retdirect_to_uint_wide()))); }
    function t_param_to_uint_wide() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint16(int16(ry))); }
    function t_local_to_uint256() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx)); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(int256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry)); }
    function t_local_to_uint256_wide() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint256(int256(rx))); }
    function t_retdirect_to_uint256_wide() external view  returns (uint256) { return uint256(uint256(int256(_s_retdirect_to_uint256_wide()))); }
    function t_param_to_uint256_wide() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_to_uint256_wide(rx); }
    function t_mem_to_uint256_wide() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint256(int256(ry))); }
    function t_local_implicit_widen() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx) + 0); }
    function t_retdirect_implicit_widen() external view  returns (uint256) { return uint256(int256(_s_retdirect_implicit_widen()) + 0); }
    function t_param_implicit_widen() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry) + 0); }
}
contract Ci24_0 {
    function _s_retdirect_to_int8() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int8(int24 pv) internal view  returns (uint256) { return uint256(int256(int8(pv))); }
    function _s_retdirect_to_int16() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int16(int24 pv) internal view  returns (uint256) { return uint256(int256(int16(pv))); }
    function _s_retdirect_to_int64() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int64(int24 pv) internal view  returns (uint256) { return uint256(int256(int64(pv))); }
    function _s_retdirect_to_int128() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int128(int24 pv) internal view  returns (uint256) { return uint256(int256(int128(pv))); }
    function _s_retdirect_to_int256() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_int256(int24 pv) internal view  returns (uint256) { return uint256(int256(int256(pv))); }
    function _s_retdirect_to_uint_same() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint_same(int24 pv) internal view  returns (uint256) { return uint256(uint24(pv)); }
    function _s_retdirect_to_uint_wide() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint_wide(int24 pv) internal view  returns (uint256) { return uint256(uint48(int48(pv))); }
    function _s_retdirect_to_uint256() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint256(int24 pv) internal view  returns (uint256) { return uint256(int256(pv)); }
    function _s_retdirect_to_uint256_wide() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_to_uint256_wide(int24 pv) internal view  returns (uint256) { return uint256(uint256(int256(pv))); }
    function _s_retdirect_implicit_widen() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_implicit_widen(int24 pv) internal view  returns (uint256) { return uint256(int256(pv) + 0); }
    function t_local_to_int8() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int8(rx))); }
    function t_retdirect_to_int8() external view  returns (uint256) { return uint256(int256(int8(_s_retdirect_to_int8()))); }
    function t_param_to_int8() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int8(rx); }
    function t_mem_to_int8() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int8(ry))); }
    function t_local_to_int16() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int16(rx))); }
    function t_retdirect_to_int16() external view  returns (uint256) { return uint256(int256(int16(_s_retdirect_to_int16()))); }
    function t_param_to_int16() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int16(rx); }
    function t_mem_to_int16() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int16(ry))); }
    function t_local_to_int64() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int64(rx))); }
    function t_retdirect_to_int64() external view  returns (uint256) { return uint256(int256(int64(_s_retdirect_to_int64()))); }
    function t_param_to_int64() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int64(rx); }
    function t_mem_to_int64() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int64(ry))); }
    function t_local_to_int128() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int128(rx))); }
    function t_retdirect_to_int128() external view  returns (uint256) { return uint256(int256(int128(_s_retdirect_to_int128()))); }
    function t_param_to_int128() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int128(rx); }
    function t_mem_to_int128() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int128(ry))); }
    function t_local_to_int256() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int256(rx))); }
    function t_retdirect_to_int256() external view  returns (uint256) { return uint256(int256(int256(_s_retdirect_to_int256()))); }
    function t_param_to_int256() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_int256(rx); }
    function t_mem_to_int256() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int256(ry))); }
    function t_local_to_uint_same() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint24(rx)); }
    function t_retdirect_to_uint_same() external view  returns (uint256) { return uint256(uint24(_s_retdirect_to_uint_same())); }
    function t_param_to_uint_same() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint_same(rx); }
    function t_mem_to_uint_same() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint24(ry)); }
    function t_local_to_uint_wide() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint48(int48(rx))); }
    function t_retdirect_to_uint_wide() external view  returns (uint256) { return uint256(uint48(int48(_s_retdirect_to_uint_wide()))); }
    function t_param_to_uint_wide() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint_wide(rx); }
    function t_mem_to_uint_wide() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint48(int48(ry))); }
    function t_local_to_uint256() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx)); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(int256(_s_retdirect_to_uint256())); }
    function t_param_to_uint256() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry)); }
    function t_local_to_uint256_wide() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint256(int256(rx))); }
    function t_retdirect_to_uint256_wide() external view  returns (uint256) { return uint256(uint256(int256(_s_retdirect_to_uint256_wide()))); }
    function t_param_to_uint256_wide() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_to_uint256_wide(rx); }
    function t_mem_to_uint256_wide() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint256(int256(ry))); }
    function t_local_implicit_widen() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx) + 0); }
    function t_retdirect_implicit_widen() external view  returns (uint256) { return uint256(int256(_s_retdirect_implicit_widen()) + 0); }
    function t_param_implicit_widen() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_implicit_widen(rx); }
    function t_mem_implicit_widen() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry) + 0); }
}
