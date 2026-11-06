//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_chk_add => 30
//@ run-call: Cu8_0::t_retdirect_chk_add => 30
//@ run-call: Cu8_0::t_ret_chk_add => 30
//@ run-call: Cu8_0::t_param_chk_add => 30
//@ run-call: Cu8_0::t_copy_chk_add => 30
//@ run-call: Cu8_0::t_ternary_chk_add => 30
//@ run-call: Cu8_0::t_ifjoin_chk_add => 30
//@ run-call: Cu8_0::t_loopjoin_chk_add => 30
//@ run-call: Cu8_0::t_tuple_chk_add => 30
//@ run-call: Cu8_0::t_modifier_chk_add => 30
//@ run-call: Cu8_0::t_ptr_chk_add => 30
//@ run-call: Cu8_0::t_mem_chk_add => 30
//@ run-call: Cu8_0::t_local_chk_add_rev => 30
//@ run-call: Cu8_0::t_retdirect_chk_add_rev => 30
//@ run-call: Cu8_0::t_ret_chk_add_rev => 30
//@ run-call: Cu8_0::t_param_chk_add_rev => 30
//@ run-call: Cu8_0::t_copy_chk_add_rev => 30
//@ run-call: Cu8_0::t_ternary_chk_add_rev => 30
//@ run-call: Cu8_0::t_ifjoin_chk_add_rev => 30
//@ run-call: Cu8_0::t_loopjoin_chk_add_rev => 30
//@ run-call: Cu8_0::t_tuple_chk_add_rev => 30
//@ run-call: Cu8_0::t_modifier_chk_add_rev => 30
//@ run-call: Cu8_0::t_ptr_chk_add_rev => 30
//@ run-call: Cu8_0::t_mem_chk_add_rev => 30
//@ run-call: Cu8_0::t_local_chk_sub => 24
//@ run-call: Cu8_0::t_retdirect_chk_sub => 24
//@ run-call: Cu8_0::t_ret_chk_sub => 24
//@ run-call: Cu8_0::t_param_chk_sub => 24
//@ run-call: Cu8_0::t_copy_chk_sub => 24
//@ run-call: Cu8_0::t_ternary_chk_sub => 24
//@ run-call: Cu8_0::t_ifjoin_chk_sub => 24
//@ run-call: Cu8_0::t_loopjoin_chk_sub => 24
//@ run-call: Cu8_0::t_tuple_chk_sub => 24
//@ run-call: Cu8_0::t_modifier_chk_sub => 24
//@ run-call: Cu8_0::t_ptr_chk_sub => 24
//@ run-call: Cu8_0::t_mem_chk_sub => 24
//@ run-call-fail: Cu8_0::t_local_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_retdirect_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_ret_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_0::t_param_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_copy_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ternary_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ifjoin_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_loopjoin_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_tuple_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_modifier_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_ptr_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu8_1::t_mem_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Cu8_1::t_local_chk_mul => 81
//@ run-call: Cu8_1::t_retdirect_chk_mul => 81
//@ run-call: Cu8_1::t_ret_chk_mul => 81
//@ run-call: Cu8_1::t_param_chk_mul => 81
//@ run-call: Cu8_1::t_copy_chk_mul => 81
//@ run-call: Cu8_1::t_ternary_chk_mul => 81
//@ run-call: Cu8_1::t_ifjoin_chk_mul => 81
//@ run-call: Cu8_1::t_loopjoin_chk_mul => 81
//@ run-call: Cu8_1::t_tuple_chk_mul => 81
//@ run-call: Cu8_1::t_modifier_chk_mul => 81
//@ run-call: Cu8_1::t_ptr_chk_mul => 81
//@ run-call: Cu8_1::t_mem_chk_mul => 81
//@ run-call: Cu8_1::t_local_chk_mul_rev => 81
//@ run-call: Cu8_1::t_retdirect_chk_mul_rev => 81
//@ run-call: Cu8_1::t_ret_chk_mul_rev => 81
//@ run-call: Cu8_1::t_param_chk_mul_rev => 81
//@ run-call: Cu8_1::t_copy_chk_mul_rev => 81
//@ run-call: Cu8_1::t_ternary_chk_mul_rev => 81
//@ run-call: Cu8_1::t_ifjoin_chk_mul_rev => 81
//@ run-call: Cu8_1::t_loopjoin_chk_mul_rev => 81
//@ run-call: Cu8_1::t_tuple_chk_mul_rev => 81
//@ run-call: Cu8_1::t_modifier_chk_mul_rev => 81
//@ run-call: Cu8_1::t_ptr_chk_mul_rev => 81
//@ run-call: Cu8_1::t_mem_chk_mul_rev => 81
//@ run-call: Cu8_1::t_local_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_retdirect_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_ret_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_param_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_copy_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_ternary_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_ifjoin_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_1::t_loopjoin_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_2::t_tuple_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_2::t_modifier_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_2::t_ptr_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_2::t_mem_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu8_2::t_local_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_retdirect_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_ret_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_param_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_copy_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_ternary_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_ifjoin_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_loopjoin_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_tuple_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_modifier_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_ptr_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_2::t_mem_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_local_chk_add => 30
//@ run-call: Cu64_0::t_retdirect_chk_add => 30
//@ run-call: Cu64_0::t_param_chk_add => 30
//@ run-call: Cu64_0::t_mem_chk_add => 30
//@ run-call: Cu64_0::t_local_chk_add_rev => 30
//@ run-call: Cu64_0::t_retdirect_chk_add_rev => 30
//@ run-call: Cu64_0::t_param_chk_add_rev => 30
//@ run-call: Cu64_0::t_mem_chk_add_rev => 30
//@ run-call: Cu64_0::t_local_chk_sub => 24
//@ run-call: Cu64_0::t_retdirect_chk_sub => 24
//@ run-call: Cu64_0::t_param_chk_sub => 24
//@ run-call: Cu64_0::t_mem_chk_sub => 24
//@ run-call-fail: Cu64_0::t_local_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu64_0::t_retdirect_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu64_0::t_param_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: Cu64_0::t_mem_chk_sub_rev => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Cu64_0::t_local_chk_mul => 81
//@ run-call: Cu64_0::t_retdirect_chk_mul => 81
//@ run-call: Cu64_0::t_param_chk_mul => 81
//@ run-call: Cu64_0::t_mem_chk_mul => 81
//@ run-call: Cu64_0::t_local_chk_mul_rev => 81
//@ run-call: Cu64_0::t_retdirect_chk_mul_rev => 81
//@ run-call: Cu64_0::t_param_chk_mul_rev => 81
//@ run-call: Cu64_0::t_mem_chk_mul_rev => 81
//@ run-call: Cu64_0::t_local_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu64_0::t_retdirect_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu64_0::t_param_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu64_0::t_mem_compound_arith => 81533206506776517519321827536926706870069178761926700817838316227314028684417
//@ run-call: Cu64_0::t_local_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_retdirect_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_param_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_mem_incdec => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Ci8_0::t_local_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_retdirect_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_param_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_mem_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_local_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_retdirect_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_param_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_mem_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci8_0::t_local_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci8_0::t_retdirect_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci8_0::t_param_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci8_0::t_mem_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci8_0::t_local_chk_sub_rev => 1
//@ run-call: Ci8_0::t_retdirect_chk_sub_rev => 1
//@ run-call: Ci8_0::t_param_chk_sub_rev => 1
//@ run-call: Ci8_0::t_mem_chk_sub_rev => 1
//@ run-call: Ci8_0::t_local_chk_mul => 6
//@ run-call: Ci8_0::t_retdirect_chk_mul => 6
//@ run-call: Ci8_0::t_param_chk_mul => 6
//@ run-call: Ci8_0::t_mem_chk_mul => 6
//@ run-call: Ci8_0::t_local_chk_mul_rev => 6
//@ run-call: Ci8_0::t_retdirect_chk_mul_rev => 6
//@ run-call: Ci8_0::t_param_chk_mul_rev => 6
//@ run-call: Ci8_0::t_mem_chk_mul_rev => 6
//@ run-call: Ci8_0::t_local_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci8_0::t_retdirect_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci8_0::t_param_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci8_0::t_mem_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci8_0::t_local_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_retdirect_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_param_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_mem_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_local_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_retdirect_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_param_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_mem_chk_add => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_local_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_retdirect_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_param_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_mem_chk_add_rev => 115792089237316195423570985008687907853269984665640564039457584007913129639931
//@ run-call: Ci24_0::t_local_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci24_0::t_retdirect_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci24_0::t_param_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci24_0::t_mem_chk_sub => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: Ci24_0::t_local_chk_sub_rev => 1
//@ run-call: Ci24_0::t_retdirect_chk_sub_rev => 1
//@ run-call: Ci24_0::t_param_chk_sub_rev => 1
//@ run-call: Ci24_0::t_mem_chk_sub_rev => 1
//@ run-call: Ci24_0::t_local_chk_mul => 6
//@ run-call: Ci24_0::t_retdirect_chk_mul => 6
//@ run-call: Ci24_0::t_param_chk_mul => 6
//@ run-call: Ci24_0::t_mem_chk_mul => 6
//@ run-call: Ci24_0::t_local_chk_mul_rev => 6
//@ run-call: Ci24_0::t_retdirect_chk_mul_rev => 6
//@ run-call: Ci24_0::t_param_chk_mul_rev => 6
//@ run-call: Ci24_0::t_mem_chk_mul_rev => 6
//@ run-call: Ci24_0::t_local_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci24_0::t_retdirect_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci24_0::t_param_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci24_0::t_mem_compound_arith => 23812345581279576455926681552524978678785917260527386338422739084616677696906
//@ run-call: Ci24_0::t_local_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_retdirect_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_param_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_mem_incdec => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Cud_0::t_local_arith => 30
//@ run-call: Cud_0::t_retdirect_arith => 30
//@ run-call: Cud_0::t_param_arith => 30
//@ run-call: Cud_0::t_mem_arith => 30

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
    function _s_retdirect_chk_add() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_add() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_add(uint8 pv) internal view  returns (uint256) { return uint256(pv + uint8(3)); }
    uint256 _res_modifier_chk_add;
    modifier _m_modifier_chk_add(uint8 pv) { _res_modifier_chk_add = uint256(pv + uint8(3)); _; }
    function _e_modifier_chk_add(uint8 rd) internal _m_modifier_chk_add(rd)  returns (uint256) { return _res_modifier_chk_add; }
    function _w_ptr_chk_add() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_chk_add_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_add_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_add_rev(uint8 pv) internal view  returns (uint256) { return uint256(uint8(3) + pv); }
    uint256 _res_modifier_chk_add_rev;
    modifier _m_modifier_chk_add_rev(uint8 pv) { _res_modifier_chk_add_rev = uint256(uint8(3) + pv); _; }
    function _e_modifier_chk_add_rev(uint8 rd) internal _m_modifier_chk_add_rev(rd)  returns (uint256) { return _res_modifier_chk_add_rev; }
    function _w_ptr_chk_add_rev() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_chk_sub() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_sub() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_sub(uint8 pv) internal view  returns (uint256) { return uint256(pv - uint8(3)); }
    uint256 _res_modifier_chk_sub;
    modifier _m_modifier_chk_sub(uint8 pv) { _res_modifier_chk_sub = uint256(pv - uint8(3)); _; }
    function _e_modifier_chk_sub(uint8 rd) internal _m_modifier_chk_sub(rd)  returns (uint256) { return _res_modifier_chk_sub; }
    function _w_ptr_chk_sub() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_chk_sub_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_sub_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_sub_rev(uint8 pv) internal view  returns (uint256) { return uint256(uint8(3) - pv); }
    function t_local_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx + uint8(3)); }
    function t_retdirect_chk_add() external view  returns (uint256) { return uint256(_s_retdirect_chk_add() + uint8(3)); }
    function t_ret_chk_add() external view  returns (uint256) { uint8 rx = _s_ret_chk_add(); return uint256(rx + uint8(3)); }
    function t_param_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_add(rx); }
    function t_copy_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry + uint8(3)); }
    function t_ternary_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry + uint8(3)); }
    function t_ifjoin_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry + uint8(3)); }
    function t_loopjoin_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry + uint8(3)); }
    function t_tuple_chk_add() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry + uint8(3)); }
    function t_modifier_chk_add() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_add(rx); }
    function t_ptr_chk_add() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_add; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry + uint8(3)); }
    function t_mem_chk_add() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry + uint8(3)); }
    function t_local_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(3) + rx); }
    function t_retdirect_chk_add_rev() external view  returns (uint256) { return uint256(uint8(3) + _s_retdirect_chk_add_rev()); }
    function t_ret_chk_add_rev() external view  returns (uint256) { uint8 rx = _s_ret_chk_add_rev(); return uint256(uint8(3) + rx); }
    function t_param_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_add_rev(rx); }
    function t_copy_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(3) + ry); }
    function t_ternary_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(3) + ry); }
    function t_ifjoin_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(3) + ry); }
    function t_loopjoin_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(3) + ry); }
    function t_tuple_chk_add_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(3) + ry); }
    function t_modifier_chk_add_rev() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_add_rev(rx); }
    function t_ptr_chk_add_rev() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_add_rev; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(3) + ry); }
    function t_mem_chk_add_rev() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(3) + ry); }
    function t_local_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx - uint8(3)); }
    function t_retdirect_chk_sub() external view  returns (uint256) { return uint256(_s_retdirect_chk_sub() - uint8(3)); }
    function t_ret_chk_sub() external view  returns (uint256) { uint8 rx = _s_ret_chk_sub(); return uint256(rx - uint8(3)); }
    function t_param_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_sub(rx); }
    function t_copy_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry - uint8(3)); }
    function t_ternary_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry - uint8(3)); }
    function t_ifjoin_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry - uint8(3)); }
    function t_loopjoin_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry - uint8(3)); }
    function t_tuple_chk_sub() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry - uint8(3)); }
    function t_modifier_chk_sub() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_sub(rx); }
    function t_ptr_chk_sub() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_sub; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry - uint8(3)); }
    function t_mem_chk_sub() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry - uint8(3)); }
    function t_local_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(3) - rx); }
    function t_retdirect_chk_sub_rev() external view  returns (uint256) { return uint256(uint8(3) - _s_retdirect_chk_sub_rev()); }
    function t_ret_chk_sub_rev() external view  returns (uint256) { uint8 rx = _s_ret_chk_sub_rev(); return uint256(uint8(3) - rx); }
    function t_param_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_sub_rev(rx); }
}
contract Cu8_1 {
    uint256 _res_modifier_chk_sub_rev;
    modifier _m_modifier_chk_sub_rev(uint8 pv) { _res_modifier_chk_sub_rev = uint256(uint8(3) - pv); _; }
    function _e_modifier_chk_sub_rev(uint8 rd) internal _m_modifier_chk_sub_rev(rd)  returns (uint256) { return _res_modifier_chk_sub_rev; }
    function _w_ptr_chk_sub_rev() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_chk_mul() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_mul() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_mul(uint8 pv) internal view  returns (uint256) { return uint256(pv * uint8(3)); }
    uint256 _res_modifier_chk_mul;
    modifier _m_modifier_chk_mul(uint8 pv) { _res_modifier_chk_mul = uint256(pv * uint8(3)); _; }
    function _e_modifier_chk_mul(uint8 rd) internal _m_modifier_chk_mul(rd)  returns (uint256) { return _res_modifier_chk_mul; }
    function _w_ptr_chk_mul() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_chk_mul_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_chk_mul_rev() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_chk_mul_rev(uint8 pv) internal view  returns (uint256) { return uint256(uint8(3) * pv); }
    uint256 _res_modifier_chk_mul_rev;
    modifier _m_modifier_chk_mul_rev(uint8 pv) { _res_modifier_chk_mul_rev = uint256(uint8(3) * pv); _; }
    function _e_modifier_chk_mul_rev(uint8 rd) internal _m_modifier_chk_mul_rev(rd)  returns (uint256) { return _res_modifier_chk_mul_rev; }
    function _w_ptr_chk_mul_rev() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_compound_arith() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_compound_arith() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_compound_arith(uint8 pv) internal view  returns (uint256) { uint8 z1 = uint8(3); z1 += pv; uint8 z2 = uint8(3); z2 *= pv; uint8 z3 = pv; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_copy_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(3) - ry); }
    function t_ternary_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(3) - ry); }
    function t_ifjoin_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(3) - ry); }
    function t_loopjoin_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(3) - ry); }
    function t_tuple_chk_sub_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(3) - ry); }
    function t_modifier_chk_sub_rev() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_sub_rev(rx); }
    function t_ptr_chk_sub_rev() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_sub_rev; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(3) - ry); }
    function t_mem_chk_sub_rev() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(3) - ry); }
    function t_local_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(rx * uint8(3)); }
    function t_retdirect_chk_mul() external view  returns (uint256) { return uint256(_s_retdirect_chk_mul() * uint8(3)); }
    function t_ret_chk_mul() external view  returns (uint256) { uint8 rx = _s_ret_chk_mul(); return uint256(rx * uint8(3)); }
    function t_param_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_mul(rx); }
    function t_copy_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(ry * uint8(3)); }
    function t_ternary_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(ry * uint8(3)); }
    function t_ifjoin_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(ry * uint8(3)); }
    function t_loopjoin_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(ry * uint8(3)); }
    function t_tuple_chk_mul() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(ry * uint8(3)); }
    function t_modifier_chk_mul() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_mul(rx); }
    function t_ptr_chk_mul() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_mul; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(ry * uint8(3)); }
    function t_mem_chk_mul() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(ry * uint8(3)); }
    function t_local_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(3) * rx); }
    function t_retdirect_chk_mul_rev() external view  returns (uint256) { return uint256(uint8(3) * _s_retdirect_chk_mul_rev()); }
    function t_ret_chk_mul_rev() external view  returns (uint256) { uint8 rx = _s_ret_chk_mul_rev(); return uint256(uint8(3) * rx); }
    function t_param_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_chk_mul_rev(rx); }
    function t_copy_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(3) * ry); }
    function t_ternary_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(3) * ry); }
    function t_ifjoin_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(3) * ry); }
    function t_loopjoin_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(3) * ry); }
    function t_tuple_chk_mul_rev() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(3) * ry); }
    function t_modifier_chk_mul_rev() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_chk_mul_rev(rx); }
    function t_ptr_chk_mul_rev() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_chk_mul_rev; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(3) * ry); }
    function t_mem_chk_mul_rev() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(3) * ry); }
    function t_local_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 z1 = uint8(3); z1 += rx; uint8 z2 = uint8(3); z2 *= rx; uint8 z3 = rx; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_arith() external view  returns (uint256) { uint8 z1 = uint8(3); z1 += _s_retdirect_compound_arith(); uint8 z2 = uint8(3); z2 *= _s_retdirect_compound_arith(); uint8 z3 = _s_retdirect_compound_arith(); z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ret_compound_arith() external view  returns (uint256) { uint8 rx = _s_ret_compound_arith(); uint8 z1 = uint8(3); z1 += rx; uint8 z2 = uint8(3); z2 *= rx; uint8 z3 = rx; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_compound_arith(rx); }
    function t_copy_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ternary_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ifjoin_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_loopjoin_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
}
contract Cu8_2 {
    uint256 _res_modifier_compound_arith;
    modifier _m_modifier_compound_arith(uint8 pv) { uint8 z1 = uint8(3); z1 += pv; uint8 z2 = uint8(3); z2 *= pv; uint8 z3 = pv; z3 %= uint8(3); _res_modifier_compound_arith = uint256(keccak256(abi.encode(z1, z2, z3))); _; }
    function _e_modifier_compound_arith(uint8 rd) internal _m_modifier_compound_arith(rd)  returns (uint256) { return _res_modifier_compound_arith; }
    function _w_ptr_compound_arith() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_incdec() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_incdec() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_incdec(uint8 pv) internal view  returns (uint256) { uint8 z1 = pv; z1++; uint8 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    uint256 _res_modifier_incdec;
    modifier _m_modifier_incdec(uint8 pv) { uint8 z1 = pv; z1++; uint8 z2 = pv; --z2; _res_modifier_incdec = uint256(keccak256(abi.encode(z1, z2))); _; }
    function _e_modifier_incdec(uint8 rd) internal _m_modifier_incdec(rd)  returns (uint256) { return _res_modifier_incdec; }
    function _w_ptr_incdec() internal pure returns (uint256) { return 0xabcd1b; }
    function t_tuple_compound_arith() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_modifier_compound_arith() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_compound_arith(rx); }
    function t_ptr_compound_arith() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_compound_arith; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_mem_compound_arith() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 z1 = uint8(3); z1 += ry; uint8 z2 = uint8(3); z2 *= ry; uint8 z3 = ry; z3 %= uint8(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 z1 = rx; z1++; uint8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_incdec() external view  returns (uint256) { uint8 z1 = _s_retdirect_incdec(); z1++; uint8 z2 = _s_retdirect_incdec(); --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ret_incdec() external view  returns (uint256) { uint8 rx = _s_ret_incdec(); uint8 z1 = rx; z1++; uint8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_incdec(rx); }
    function t_copy_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ternary_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ifjoin_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_loopjoin_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_tuple_incdec() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_modifier_incdec() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_incdec(rx); }
    function t_ptr_incdec() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_incdec; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_mem_incdec() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Cu64_0 {
    function _s_retdirect_chk_add() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_add(uint64 pv) internal view  returns (uint256) { return uint256(pv + uint64(3)); }
    function _s_retdirect_chk_add_rev() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_add_rev(uint64 pv) internal view  returns (uint256) { return uint256(uint64(3) + pv); }
    function _s_retdirect_chk_sub() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_sub(uint64 pv) internal view  returns (uint256) { return uint256(pv - uint64(3)); }
    function _s_retdirect_chk_sub_rev() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_sub_rev(uint64 pv) internal view  returns (uint256) { return uint256(uint64(3) - pv); }
    function _s_retdirect_chk_mul() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_mul(uint64 pv) internal view  returns (uint256) { return uint256(pv * uint64(3)); }
    function _s_retdirect_chk_mul_rev() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_chk_mul_rev(uint64 pv) internal view  returns (uint256) { return uint256(uint64(3) * pv); }
    function _s_retdirect_compound_arith() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_compound_arith(uint64 pv) internal view  returns (uint256) { uint64 z1 = uint64(3); z1 += pv; uint64 z2 = uint64(3); z2 *= pv; uint64 z3 = pv; z3 %= uint64(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function _s_retdirect_incdec() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_incdec(uint64 pv) internal view  returns (uint256) { uint64 z1 = pv; z1++; uint64 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_local_chk_add() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx + uint64(3)); }
    function t_retdirect_chk_add() external view  returns (uint256) { return uint256(_s_retdirect_chk_add() + uint64(3)); }
    function t_param_chk_add() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_add(rx); }
    function t_mem_chk_add() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry + uint64(3)); }
    function t_local_chk_add_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(3) + rx); }
    function t_retdirect_chk_add_rev() external view  returns (uint256) { return uint256(uint64(3) + _s_retdirect_chk_add_rev()); }
    function t_param_chk_add_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_add_rev(rx); }
    function t_mem_chk_add_rev() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(3) + ry); }
    function t_local_chk_sub() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx - uint64(3)); }
    function t_retdirect_chk_sub() external view  returns (uint256) { return uint256(_s_retdirect_chk_sub() - uint64(3)); }
    function t_param_chk_sub() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_sub(rx); }
    function t_mem_chk_sub() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry - uint64(3)); }
    function t_local_chk_sub_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(3) - rx); }
    function t_retdirect_chk_sub_rev() external view  returns (uint256) { return uint256(uint64(3) - _s_retdirect_chk_sub_rev()); }
    function t_param_chk_sub_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_sub_rev(rx); }
    function t_mem_chk_sub_rev() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(3) - ry); }
    function t_local_chk_mul() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(rx * uint64(3)); }
    function t_retdirect_chk_mul() external view  returns (uint256) { return uint256(_s_retdirect_chk_mul() * uint64(3)); }
    function t_param_chk_mul() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_mul(rx); }
    function t_mem_chk_mul() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(ry * uint64(3)); }
    function t_local_chk_mul_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(3) * rx); }
    function t_retdirect_chk_mul_rev() external view  returns (uint256) { return uint256(uint64(3) * _s_retdirect_chk_mul_rev()); }
    function t_param_chk_mul_rev() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_chk_mul_rev(rx); }
    function t_mem_chk_mul_rev() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(3) * ry); }
    function t_local_compound_arith() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 z1 = uint64(3); z1 += rx; uint64 z2 = uint64(3); z2 *= rx; uint64 z3 = rx; z3 %= uint64(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_arith() external view  returns (uint256) { uint64 z1 = uint64(3); z1 += _s_retdirect_compound_arith(); uint64 z2 = uint64(3); z2 *= _s_retdirect_compound_arith(); uint64 z3 = _s_retdirect_compound_arith(); z3 %= uint64(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_arith() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_compound_arith(rx); }
    function t_mem_compound_arith() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 z1 = uint64(3); z1 += ry; uint64 z2 = uint64(3); z2 *= ry; uint64 z3 = ry; z3 %= uint64(3); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_incdec() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 z1 = rx; z1++; uint64 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_incdec() external view  returns (uint256) { uint64 z1 = _s_retdirect_incdec(); z1++; uint64 z2 = _s_retdirect_incdec(); --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_incdec() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_incdec(rx); }
    function t_mem_incdec() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 z1 = ry; z1++; uint64 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Ci8_0 {
    function _s_retdirect_chk_add() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_add(int8 pv) internal view  returns (uint256) { return uint256(int256(pv + int8(-2))); }
    function _s_retdirect_chk_add_rev() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_add_rev(int8 pv) internal view  returns (uint256) { return uint256(int256(int8(-2) + pv)); }
    function _s_retdirect_chk_sub() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_sub(int8 pv) internal view  returns (uint256) { return uint256(int256(pv - int8(-2))); }
    function _s_retdirect_chk_sub_rev() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_sub_rev(int8 pv) internal view  returns (uint256) { return uint256(int256(int8(-2) - pv)); }
    function _s_retdirect_chk_mul() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_mul(int8 pv) internal view  returns (uint256) { return uint256(int256(pv * int8(-2))); }
    function _s_retdirect_chk_mul_rev() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_chk_mul_rev(int8 pv) internal view  returns (uint256) { return uint256(int256(int8(-2) * pv)); }
    function _s_retdirect_compound_arith() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_compound_arith(int8 pv) internal view  returns (uint256) { int8 z1 = int8(-2); z1 += pv; int8 z2 = int8(-2); z2 *= pv; int8 z3 = pv; z3 %= int8(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function _s_retdirect_incdec() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_incdec(int8 pv) internal view  returns (uint256) { int8 z1 = pv; z1++; int8 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_local_chk_add() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx + int8(-2))); }
    function t_retdirect_chk_add() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_add() + int8(-2))); }
    function t_param_chk_add() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_add(rx); }
    function t_mem_chk_add() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry + int8(-2))); }
    function t_local_chk_add_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int8(-2) + rx)); }
    function t_retdirect_chk_add_rev() external view  returns (uint256) { return uint256(int256(int8(-2) + _s_retdirect_chk_add_rev())); }
    function t_param_chk_add_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_add_rev(rx); }
    function t_mem_chk_add_rev() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int8(-2) + ry)); }
    function t_local_chk_sub() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx - int8(-2))); }
    function t_retdirect_chk_sub() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_sub() - int8(-2))); }
    function t_param_chk_sub() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_sub(rx); }
    function t_mem_chk_sub() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry - int8(-2))); }
    function t_local_chk_sub_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int8(-2) - rx)); }
    function t_retdirect_chk_sub_rev() external view  returns (uint256) { return uint256(int256(int8(-2) - _s_retdirect_chk_sub_rev())); }
    function t_param_chk_sub_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_sub_rev(rx); }
    function t_mem_chk_sub_rev() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int8(-2) - ry)); }
    function t_local_chk_mul() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(rx * int8(-2))); }
    function t_retdirect_chk_mul() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_mul() * int8(-2))); }
    function t_param_chk_mul() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_mul(rx); }
    function t_mem_chk_mul() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(ry * int8(-2))); }
    function t_local_chk_mul_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(int256(int8(-2) * rx)); }
    function t_retdirect_chk_mul_rev() external view  returns (uint256) { return uint256(int256(int8(-2) * _s_retdirect_chk_mul_rev())); }
    function t_param_chk_mul_rev() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_chk_mul_rev(rx); }
    function t_mem_chk_mul_rev() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(int256(int8(-2) * ry)); }
    function t_local_compound_arith() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 z1 = int8(-2); z1 += rx; int8 z2 = int8(-2); z2 *= rx; int8 z3 = rx; z3 %= int8(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_arith() external view  returns (uint256) { int8 z1 = int8(-2); z1 += _s_retdirect_compound_arith(); int8 z2 = int8(-2); z2 *= _s_retdirect_compound_arith(); int8 z3 = _s_retdirect_compound_arith(); z3 %= int8(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_arith() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_compound_arith(rx); }
    function t_mem_compound_arith() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 z1 = int8(-2); z1 += ry; int8 z2 = int8(-2); z2 *= ry; int8 z3 = ry; z3 %= int8(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_incdec() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 z1 = rx; z1++; int8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_incdec() external view  returns (uint256) { int8 z1 = _s_retdirect_incdec(); z1++; int8 z2 = _s_retdirect_incdec(); --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_incdec() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_incdec(rx); }
    function t_mem_incdec() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 z1 = ry; z1++; int8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Ci24_0 {
    function _s_retdirect_chk_add() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_add(int24 pv) internal view  returns (uint256) { return uint256(int256(pv + int24(-2))); }
    function _s_retdirect_chk_add_rev() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_add_rev(int24 pv) internal view  returns (uint256) { return uint256(int256(int24(-2) + pv)); }
    function _s_retdirect_chk_sub() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_sub(int24 pv) internal view  returns (uint256) { return uint256(int256(pv - int24(-2))); }
    function _s_retdirect_chk_sub_rev() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_sub_rev(int24 pv) internal view  returns (uint256) { return uint256(int256(int24(-2) - pv)); }
    function _s_retdirect_chk_mul() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_mul(int24 pv) internal view  returns (uint256) { return uint256(int256(pv * int24(-2))); }
    function _s_retdirect_chk_mul_rev() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_chk_mul_rev(int24 pv) internal view  returns (uint256) { return uint256(int256(int24(-2) * pv)); }
    function _s_retdirect_compound_arith() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_compound_arith(int24 pv) internal view  returns (uint256) { int24 z1 = int24(-2); z1 += pv; int24 z2 = int24(-2); z2 *= pv; int24 z3 = pv; z3 %= int24(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function _s_retdirect_incdec() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_incdec(int24 pv) internal view  returns (uint256) { int24 z1 = pv; z1++; int24 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_local_chk_add() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx + int24(-2))); }
    function t_retdirect_chk_add() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_add() + int24(-2))); }
    function t_param_chk_add() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_add(rx); }
    function t_mem_chk_add() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry + int24(-2))); }
    function t_local_chk_add_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int24(-2) + rx)); }
    function t_retdirect_chk_add_rev() external view  returns (uint256) { return uint256(int256(int24(-2) + _s_retdirect_chk_add_rev())); }
    function t_param_chk_add_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_add_rev(rx); }
    function t_mem_chk_add_rev() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int24(-2) + ry)); }
    function t_local_chk_sub() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx - int24(-2))); }
    function t_retdirect_chk_sub() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_sub() - int24(-2))); }
    function t_param_chk_sub() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_sub(rx); }
    function t_mem_chk_sub() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry - int24(-2))); }
    function t_local_chk_sub_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int24(-2) - rx)); }
    function t_retdirect_chk_sub_rev() external view  returns (uint256) { return uint256(int256(int24(-2) - _s_retdirect_chk_sub_rev())); }
    function t_param_chk_sub_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_sub_rev(rx); }
    function t_mem_chk_sub_rev() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int24(-2) - ry)); }
    function t_local_chk_mul() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(rx * int24(-2))); }
    function t_retdirect_chk_mul() external view  returns (uint256) { return uint256(int256(_s_retdirect_chk_mul() * int24(-2))); }
    function t_param_chk_mul() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_mul(rx); }
    function t_mem_chk_mul() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(ry * int24(-2))); }
    function t_local_chk_mul_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(int256(int24(-2) * rx)); }
    function t_retdirect_chk_mul_rev() external view  returns (uint256) { return uint256(int256(int24(-2) * _s_retdirect_chk_mul_rev())); }
    function t_param_chk_mul_rev() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_chk_mul_rev(rx); }
    function t_mem_chk_mul_rev() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(int256(int24(-2) * ry)); }
    function t_local_compound_arith() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 z1 = int24(-2); z1 += rx; int24 z2 = int24(-2); z2 *= rx; int24 z3 = rx; z3 %= int24(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_arith() external view  returns (uint256) { int24 z1 = int24(-2); z1 += _s_retdirect_compound_arith(); int24 z2 = int24(-2); z2 *= _s_retdirect_compound_arith(); int24 z3 = _s_retdirect_compound_arith(); z3 %= int24(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_arith() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_compound_arith(rx); }
    function t_mem_compound_arith() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 z1 = int24(-2); z1 += ry; int24 z2 = int24(-2); z2 *= ry; int24 z3 = ry; z3 %= int24(-2); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_incdec() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 z1 = rx; z1++; int24 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_incdec() external view  returns (uint256) { int24 z1 = _s_retdirect_incdec(); z1++; int24 z2 = _s_retdirect_incdec(); --z2; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_incdec() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_incdec(rx); }
    function t_mem_incdec() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 z1 = ry; z1++; int24 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Cud_0 {
    function _s_retdirect_arith() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_arith(U pv) internal view  returns (uint256) { return uint256(U.unwrap(pv) + U.unwrap(U.wrap(3))); }
    function t_local_arith() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(rx) + U.unwrap(U.wrap(3))); }
    function t_retdirect_arith() external view  returns (uint256) { return uint256(U.unwrap(_s_retdirect_arith()) + U.unwrap(U.wrap(3))); }
    function t_param_arith() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_arith(rx); }
    function t_mem_arith() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(ry) + U.unwrap(U.wrap(3))); }
}
