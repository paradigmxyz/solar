//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_retdirect_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_ret_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_param_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_copy_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_ternary_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_ifjoin_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_loopjoin_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_tuple_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_modifier_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_ptr_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_mem_bitwise_all => 102487998326893221141726897299857230510375897942360391020046694045655357652947
//@ run-call: Cu8_0::t_local_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_retdirect_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_ret_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_param_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_copy_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_ternary_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_ifjoin_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_loopjoin_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_tuple_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_modifier_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_ptr_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu8_0::t_mem_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu64_0::t_local_bitwise_all => 43089287993077170983442676002302243436692628970215841770635724249842084575283
//@ run-call: Cu64_0::t_retdirect_bitwise_all => 43089287993077170983442676002302243436692628970215841770635724249842084575283
//@ run-call: Cu64_0::t_param_bitwise_all => 43089287993077170983442676002302243436692628970215841770635724249842084575283
//@ run-call: Cu64_0::t_mem_bitwise_all => 43089287993077170983442676002302243436692628970215841770635724249842084575283
//@ run-call: Cu64_0::t_local_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu64_0::t_retdirect_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu64_0::t_param_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Cu64_0::t_mem_compound_bits => 65409061692162195513629329505231690236366341442682518110244220746532783642330
//@ run-call: Ci8_0::t_local_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci8_0::t_retdirect_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci8_0::t_param_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci8_0::t_mem_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci8_0::t_local_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci8_0::t_retdirect_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci8_0::t_param_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci8_0::t_mem_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci24_0::t_local_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci24_0::t_retdirect_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci24_0::t_param_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci24_0::t_mem_bitwise_all => 21002109069422963810566805903065347299710071924565598393408750862730437049031
//@ run-call: Ci24_0::t_local_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci24_0::t_retdirect_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci24_0::t_param_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Ci24_0::t_mem_compound_bits => 86206629113771335852286458931744536561241367777517719860160630656700763764515
//@ run-call: Cbool_0::t_local_not => 0
//@ run-call: Cbool_0::t_retdirect_not => 0
//@ run-call: Cbool_0::t_param_not => 0
//@ run-call: Cbool_0::t_mem_not => 0
//@ run-call: Cbool_0::t_local_and => 0
//@ run-call: Cbool_0::t_retdirect_and => 0
//@ run-call: Cbool_0::t_param_and => 0
//@ run-call: Cbool_0::t_mem_and => 0
//@ run-call: Cbool_0::t_local_or => 1
//@ run-call: Cbool_0::t_retdirect_or => 1
//@ run-call: Cbool_0::t_param_or => 1
//@ run-call: Cbool_0::t_mem_or => 1
//@ run-call: Cbool_0::t_local_or_rev => 1
//@ run-call: Cbool_0::t_retdirect_or_rev => 1
//@ run-call: Cbool_0::t_param_or_rev => 1
//@ run-call: Cbool_0::t_mem_or_rev => 1
//@ run-call: Cb1_0::t_local_and => 137
//@ run-call: Cb1_0::t_retdirect_and => 137
//@ run-call: Cb1_0::t_param_and => 137
//@ run-call: Cb1_0::t_mem_and => 137
//@ run-call: Cb1_0::t_local_or => 239
//@ run-call: Cb1_0::t_retdirect_or => 239
//@ run-call: Cb1_0::t_param_or => 239
//@ run-call: Cb1_0::t_mem_or => 239
//@ run-call: Cb1_0::t_local_xor => 102
//@ run-call: Cb1_0::t_retdirect_xor => 102
//@ run-call: Cb1_0::t_param_xor => 102
//@ run-call: Cb1_0::t_mem_xor => 102
//@ run-call: Cb1_0::t_local_not => 84
//@ run-call: Cb1_0::t_retdirect_not => 84
//@ run-call: Cb1_0::t_param_not => 84
//@ run-call: Cb1_0::t_mem_not => 84
//@ run-call: Cb1_0::t_local_cmpd_or => 239
//@ run-call: Cb1_0::t_retdirect_cmpd_or => 239
//@ run-call: Cb1_0::t_param_cmpd_or => 239
//@ run-call: Cb1_0::t_mem_cmpd_or => 239
//@ run-call: Cb4_0::t_local_and => 270545476
//@ run-call: Cb4_0::t_retdirect_and => 270545476
//@ run-call: Cb4_0::t_ret_and => 270545476
//@ run-call: Cb4_0::t_param_and => 270545476
//@ run-call: Cb4_0::t_copy_and => 270545476
//@ run-call: Cb4_0::t_ternary_and => 270545476
//@ run-call: Cb4_0::t_ifjoin_and => 270545476
//@ run-call: Cb4_0::t_loopjoin_and => 270545476
//@ run-call: Cb4_0::t_tuple_and => 270545476
//@ run-call: Cb4_0::t_modifier_and => 270545476
//@ run-call: Cb4_0::t_ptr_and => 270545476
//@ run-call: Cb4_0::t_mem_and => 270545476
//@ run-call: Cb4_0::t_local_or => 3752837103
//@ run-call: Cb4_0::t_retdirect_or => 3752837103
//@ run-call: Cb4_0::t_ret_or => 3752837103
//@ run-call: Cb4_0::t_param_or => 3752837103
//@ run-call: Cb4_0::t_copy_or => 3752837103
//@ run-call: Cb4_0::t_ternary_or => 3752837103
//@ run-call: Cb4_0::t_ifjoin_or => 3752837103
//@ run-call: Cb4_0::t_loopjoin_or => 3752837103
//@ run-call: Cb4_0::t_tuple_or => 3752837103
//@ run-call: Cb4_0::t_modifier_or => 3752837103
//@ run-call: Cb4_0::t_ptr_or => 3752837103
//@ run-call: Cb4_0::t_mem_or => 3752837103
//@ run-call: Cb4_0::t_local_xor => 3482291627
//@ run-call: Cb4_0::t_retdirect_xor => 3482291627
//@ run-call: Cb4_0::t_ret_xor => 3482291627
//@ run-call: Cb4_0::t_param_xor => 3482291627
//@ run-call: Cb4_0::t_copy_xor => 3482291627
//@ run-call: Cb4_0::t_ternary_xor => 3482291627
//@ run-call: Cb4_0::t_ifjoin_xor => 3482291627
//@ run-call: Cb4_0::t_loopjoin_xor => 3482291627
//@ run-call: Cb4_0::t_tuple_xor => 3482291627
//@ run-call: Cb4_0::t_modifier_xor => 3482291627
//@ run-call: Cb4_0::t_ptr_xor => 3482291627
//@ run-call: Cb4_0::t_mem_xor => 3482291627
//@ run-call: Cb4_0::t_local_not => 559038736
//@ run-call: Cb4_0::t_retdirect_not => 559038736
//@ run-call: Cb4_0::t_ret_not => 559038736
//@ run-call: Cb4_0::t_param_not => 559038736
//@ run-call: Cb4_1::t_copy_not => 559038736
//@ run-call: Cb4_1::t_ternary_not => 559038736
//@ run-call: Cb4_1::t_ifjoin_not => 559038736
//@ run-call: Cb4_1::t_loopjoin_not => 559038736
//@ run-call: Cb4_1::t_tuple_not => 559038736
//@ run-call: Cb4_1::t_modifier_not => 559038736
//@ run-call: Cb4_1::t_ptr_not => 559038736
//@ run-call: Cb4_1::t_mem_not => 559038736
//@ run-call: Cb4_1::t_local_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_retdirect_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_ret_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_param_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_copy_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_ternary_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_ifjoin_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_loopjoin_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_tuple_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_modifier_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_ptr_cmpd_or => 3752837103
//@ run-call: Cb4_1::t_mem_cmpd_or => 3752837103
//@ run-call: Cb20_0::t_local_and => 91702063518801751730427283621413836527431061520
//@ run-call: Cb20_0::t_retdirect_and => 91702063518801751730427283621413836527431061520
//@ run-call: Cb20_0::t_param_and => 91702063518801751730427283621413836527431061520
//@ run-call: Cb20_0::t_mem_and => 91702063518801751730427283621413836527431061520
//@ run-call: Cb20_0::t_local_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_retdirect_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_param_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_mem_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_local_xor => 17958320758253577758823345189477033237644011905
//@ run-call: Cb20_0::t_retdirect_xor => 17958320758253577758823345189477033237644011905
//@ run-call: Cb20_0::t_param_xor => 17958320758253577758823345189477033237644011905
//@ run-call: Cb20_0::t_mem_xor => 17958320758253577758823345189477033237644011905
//@ run-call: Cb20_0::t_local_not => 1357572632023772698197585909131730514673821910895
//@ run-call: Cb20_0::t_retdirect_not => 1357572632023772698197585909131730514673821910895
//@ run-call: Cb20_0::t_param_not => 1357572632023772698197585909131730514673821910895
//@ run-call: Cb20_0::t_mem_not => 1357572632023772698197585909131730514673821910895
//@ run-call: Cb20_0::t_local_cmpd_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_retdirect_cmpd_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_param_cmpd_or => 109660384277055329489250628810890869765075073425
//@ run-call: Cb20_0::t_mem_cmpd_or => 109660384277055329489250628810890869765075073425

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
    function _s_retdirect_bitwise_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_bitwise_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_bitwise_all(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv & uint8(3), pv | uint8(3), pv ^ uint8(3), ~pv))); }
    uint256 _res_modifier_bitwise_all;
    modifier _m_modifier_bitwise_all(uint8 pv) { _res_modifier_bitwise_all = uint256(keccak256(abi.encode(pv & uint8(3), pv | uint8(3), pv ^ uint8(3), ~pv))); _; }
    function _e_modifier_bitwise_all(uint8 rd) internal _m_modifier_bitwise_all(rd)  returns (uint256) { return _res_modifier_bitwise_all; }
    function _w_ptr_bitwise_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_compound_bits() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_compound_bits() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_compound_bits(uint8 pv) internal view  returns (uint256) { uint8 z1 = uint8(3); z1 |= pv; uint8 z2 = uint8(3); z2 &= pv; uint8 z3 = uint8(3); z3 ^= pv; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    uint256 _res_modifier_compound_bits;
    modifier _m_modifier_compound_bits(uint8 pv) { uint8 z1 = uint8(3); z1 |= pv; uint8 z2 = uint8(3); z2 &= pv; uint8 z3 = uint8(3); z3 ^= pv; _res_modifier_compound_bits = uint256(keccak256(abi.encode(z1, z2, z3))); _; }
    function _e_modifier_compound_bits(uint8 rd) internal _m_modifier_compound_bits(rd)  returns (uint256) { return _res_modifier_compound_bits; }
    function _w_ptr_compound_bits() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx & uint8(3), rx | uint8(3), rx ^ uint8(3), ~rx))); }
    function t_retdirect_bitwise_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_bitwise_all() & uint8(3), _s_retdirect_bitwise_all() | uint8(3), _s_retdirect_bitwise_all() ^ uint8(3), ~_s_retdirect_bitwise_all()))); }
    function t_ret_bitwise_all() external view  returns (uint256) { uint8 rx = _s_ret_bitwise_all(); return uint256(keccak256(abi.encode(rx & uint8(3), rx | uint8(3), rx ^ uint8(3), ~rx))); }
    function t_param_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_bitwise_all(rx); }
    function t_copy_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_ternary_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_ifjoin_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_loopjoin_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_tuple_bitwise_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_modifier_bitwise_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_bitwise_all(rx); }
    function t_ptr_bitwise_all() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_bitwise_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_mem_bitwise_all() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(ry & uint8(3), ry | uint8(3), ry ^ uint8(3), ~ry))); }
    function t_local_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 z1 = uint8(3); z1 |= rx; uint8 z2 = uint8(3); z2 &= rx; uint8 z3 = uint8(3); z3 ^= rx; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_bits() external view  returns (uint256) { uint8 z1 = uint8(3); z1 |= _s_retdirect_compound_bits(); uint8 z2 = uint8(3); z2 &= _s_retdirect_compound_bits(); uint8 z3 = uint8(3); z3 ^= _s_retdirect_compound_bits(); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ret_compound_bits() external view  returns (uint256) { uint8 rx = _s_ret_compound_bits(); uint8 z1 = uint8(3); z1 |= rx; uint8 z2 = uint8(3); z2 &= rx; uint8 z3 = uint8(3); z3 ^= rx; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_compound_bits(rx); }
    function t_copy_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ternary_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_ifjoin_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_loopjoin_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_tuple_compound_bits() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_modifier_compound_bits() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_compound_bits(rx); }
    function t_ptr_compound_bits() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_compound_bits; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_mem_compound_bits() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 z1 = uint8(3); z1 |= ry; uint8 z2 = uint8(3); z2 &= ry; uint8 z3 = uint8(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
}
contract Cu64_0 {
    function _s_retdirect_bitwise_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_bitwise_all(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv & uint64(3), pv | uint64(3), pv ^ uint64(3), ~pv))); }
    function _s_retdirect_compound_bits() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_compound_bits(uint64 pv) internal view  returns (uint256) { uint64 z1 = uint64(3); z1 |= pv; uint64 z2 = uint64(3); z2 &= pv; uint64 z3 = uint64(3); z3 ^= pv; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_bitwise_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(rx & uint64(3), rx | uint64(3), rx ^ uint64(3), ~rx))); }
    function t_retdirect_bitwise_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_bitwise_all() & uint64(3), _s_retdirect_bitwise_all() | uint64(3), _s_retdirect_bitwise_all() ^ uint64(3), ~_s_retdirect_bitwise_all()))); }
    function t_param_bitwise_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_bitwise_all(rx); }
    function t_mem_bitwise_all() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(ry & uint64(3), ry | uint64(3), ry ^ uint64(3), ~ry))); }
    function t_local_compound_bits() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 z1 = uint64(3); z1 |= rx; uint64 z2 = uint64(3); z2 &= rx; uint64 z3 = uint64(3); z3 ^= rx; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_bits() external view  returns (uint256) { uint64 z1 = uint64(3); z1 |= _s_retdirect_compound_bits(); uint64 z2 = uint64(3); z2 &= _s_retdirect_compound_bits(); uint64 z3 = uint64(3); z3 ^= _s_retdirect_compound_bits(); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_bits() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_compound_bits(rx); }
    function t_mem_compound_bits() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 z1 = uint64(3); z1 |= ry; uint64 z2 = uint64(3); z2 &= ry; uint64 z3 = uint64(3); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
}
contract Ci8_0 {
    function _s_retdirect_bitwise_all() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_bitwise_all(int8 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv & int8(-2), pv | int8(-2), pv ^ int8(-2), ~pv, -pv))); } }
    function _s_retdirect_compound_bits() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_compound_bits(int8 pv) internal view  returns (uint256) { int8 z1 = int8(-2); z1 |= pv; int8 z2 = int8(-2); z2 &= pv; int8 z3 = int8(-2); z3 ^= pv; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_bitwise_all() external view  returns (uint256) { unchecked { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(rx & int8(-2), rx | int8(-2), rx ^ int8(-2), ~rx, -rx))); } }
    function t_retdirect_bitwise_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_bitwise_all() & int8(-2), _s_retdirect_bitwise_all() | int8(-2), _s_retdirect_bitwise_all() ^ int8(-2), ~_s_retdirect_bitwise_all(), -_s_retdirect_bitwise_all()))); } }
    function t_param_bitwise_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_bitwise_all(rx); }
    function t_mem_bitwise_all() external view  returns (uint256) { unchecked { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(ry & int8(-2), ry | int8(-2), ry ^ int8(-2), ~ry, -ry))); } }
    function t_local_compound_bits() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 z1 = int8(-2); z1 |= rx; int8 z2 = int8(-2); z2 &= rx; int8 z3 = int8(-2); z3 ^= rx; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_bits() external view  returns (uint256) { int8 z1 = int8(-2); z1 |= _s_retdirect_compound_bits(); int8 z2 = int8(-2); z2 &= _s_retdirect_compound_bits(); int8 z3 = int8(-2); z3 ^= _s_retdirect_compound_bits(); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_bits() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_compound_bits(rx); }
    function t_mem_compound_bits() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 z1 = int8(-2); z1 |= ry; int8 z2 = int8(-2); z2 &= ry; int8 z3 = int8(-2); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
}
contract Ci24_0 {
    function _s_retdirect_bitwise_all() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_bitwise_all(int24 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv & int24(-2), pv | int24(-2), pv ^ int24(-2), ~pv, -pv))); } }
    function _s_retdirect_compound_bits() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_compound_bits(int24 pv) internal view  returns (uint256) { int24 z1 = int24(-2); z1 |= pv; int24 z2 = int24(-2); z2 &= pv; int24 z3 = int24(-2); z3 ^= pv; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_local_bitwise_all() external view  returns (uint256) { unchecked { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(rx & int24(-2), rx | int24(-2), rx ^ int24(-2), ~rx, -rx))); } }
    function t_retdirect_bitwise_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_bitwise_all() & int24(-2), _s_retdirect_bitwise_all() | int24(-2), _s_retdirect_bitwise_all() ^ int24(-2), ~_s_retdirect_bitwise_all(), -_s_retdirect_bitwise_all()))); } }
    function t_param_bitwise_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_bitwise_all(rx); }
    function t_mem_bitwise_all() external view  returns (uint256) { unchecked { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(ry & int24(-2), ry | int24(-2), ry ^ int24(-2), ~ry, -ry))); } }
    function t_local_compound_bits() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 z1 = int24(-2); z1 |= rx; int24 z2 = int24(-2); z2 &= rx; int24 z3 = int24(-2); z3 ^= rx; return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_retdirect_compound_bits() external view  returns (uint256) { int24 z1 = int24(-2); z1 |= _s_retdirect_compound_bits(); int24 z2 = int24(-2); z2 &= _s_retdirect_compound_bits(); int24 z3 = int24(-2); z3 ^= _s_retdirect_compound_bits(); return uint256(keccak256(abi.encode(z1, z2, z3))); }
    function t_param_compound_bits() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_compound_bits(rx); }
    function t_mem_compound_bits() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 z1 = int24(-2); z1 |= ry; int24 z2 = int24(-2); z2 &= ry; int24 z3 = int24(-2); z3 ^= ry; return uint256(keccak256(abi.encode(z1, z2, z3))); }
}
contract Cbool_0 {
    function _s_retdirect_not() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_not(bool pv) internal view  returns (uint256) { return (!pv) ? 1 : 0; }
    function _s_retdirect_and() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_and(bool pv) internal view  returns (uint256) { return (pv && false) ? 1 : 0; }
    function _s_retdirect_or() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_or(bool pv) internal view  returns (uint256) { return (pv || false) ? 1 : 0; }
    function _s_retdirect_or_rev() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_or_rev(bool pv) internal view  returns (uint256) { return (false || pv) ? 1 : 0; }
    function t_local_not() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (!rx) ? 1 : 0; }
    function t_retdirect_not() external view  returns (uint256) { return (!_s_retdirect_not()) ? 1 : 0; }
    function t_param_not() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_not(rx); }
    function t_mem_not() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (!ry) ? 1 : 0; }
    function t_local_and() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (rx && false) ? 1 : 0; }
    function t_retdirect_and() external view  returns (uint256) { return (_s_retdirect_and() && false) ? 1 : 0; }
    function t_param_and() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_and(rx); }
    function t_mem_and() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (ry && false) ? 1 : 0; }
    function t_local_or() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (rx || false) ? 1 : 0; }
    function t_retdirect_or() external view  returns (uint256) { return (_s_retdirect_or() || false) ? 1 : 0; }
    function t_param_or() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_or(rx); }
    function t_mem_or() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (ry || false) ? 1 : 0; }
    function t_local_or_rev() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return (false || rx) ? 1 : 0; }
    function t_retdirect_or_rev() external view  returns (uint256) { return (false || _s_retdirect_or_rev()) ? 1 : 0; }
    function t_param_or_rev() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_or_rev(rx); }
    function t_mem_or_rev() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return (false || ry) ? 1 : 0; }
}
contract Cb1_0 {
    function _s_retdirect_and() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_and(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv & bytes1(0xcd))); }
    function _s_retdirect_or() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_or(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv | bytes1(0xcd))); }
    function _s_retdirect_xor() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_xor(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv ^ bytes1(0xcd))); }
    function _s_retdirect_not() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_not(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(~pv)); }
    function _s_retdirect_cmpd_or() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmpd_or(bytes1 pv) internal view  returns (uint256) { bytes1 z = bytes1(0xcd); z |= pv; return uint256(uint8(z)); }
    function t_local_and() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx & bytes1(0xcd))); }
    function t_retdirect_and() external view  returns (uint256) { return uint256(uint8(_s_retdirect_and() & bytes1(0xcd))); }
    function t_param_and() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_and(rx); }
    function t_mem_and() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry & bytes1(0xcd))); }
    function t_local_or() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx | bytes1(0xcd))); }
    function t_retdirect_or() external view  returns (uint256) { return uint256(uint8(_s_retdirect_or() | bytes1(0xcd))); }
    function t_param_or() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_or(rx); }
    function t_mem_or() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry | bytes1(0xcd))); }
    function t_local_xor() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx ^ bytes1(0xcd))); }
    function t_retdirect_xor() external view  returns (uint256) { return uint256(uint8(_s_retdirect_xor() ^ bytes1(0xcd))); }
    function t_param_xor() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_xor(rx); }
    function t_mem_xor() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry ^ bytes1(0xcd))); }
    function t_local_not() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(~rx)); }
    function t_retdirect_not() external view  returns (uint256) { return uint256(uint8(~_s_retdirect_not())); }
    function t_param_not() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_not(rx); }
    function t_mem_not() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(~ry)); }
    function t_local_cmpd_or() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 z = bytes1(0xcd); z |= rx; return uint256(uint8(z)); }
    function t_retdirect_cmpd_or() external view  returns (uint256) { bytes1 z = bytes1(0xcd); z |= _s_retdirect_cmpd_or(); return uint256(uint8(z)); }
    function t_param_cmpd_or() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmpd_or(rx); }
    function t_mem_cmpd_or() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 z = bytes1(0xcd); z |= ry; return uint256(uint8(z)); }
}
contract Cb4_0 {
    function _s_retdirect_and() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_and() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_and(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv & bytes4(0x11223344))); }
    uint256 _res_modifier_and;
    modifier _m_modifier_and(bytes4 pv) { _res_modifier_and = uint256(uint32(pv & bytes4(0x11223344))); _; }
    function _e_modifier_and(bytes4 rd) internal _m_modifier_and(rd)  returns (uint256) { return _res_modifier_and; }
    function _w_ptr_and() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_or() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_or() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_or(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv | bytes4(0x11223344))); }
    uint256 _res_modifier_or;
    modifier _m_modifier_or(bytes4 pv) { _res_modifier_or = uint256(uint32(pv | bytes4(0x11223344))); _; }
    function _e_modifier_or(bytes4 rd) internal _m_modifier_or(rd)  returns (uint256) { return _res_modifier_or; }
    function _w_ptr_or() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_xor() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_xor() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_xor(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv ^ bytes4(0x11223344))); }
    uint256 _res_modifier_xor;
    modifier _m_modifier_xor(bytes4 pv) { _res_modifier_xor = uint256(uint32(pv ^ bytes4(0x11223344))); _; }
    function _e_modifier_xor(bytes4 rd) internal _m_modifier_xor(rd)  returns (uint256) { return _res_modifier_xor; }
    function _w_ptr_xor() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_not() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_not() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_not(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(~pv)); }
    function t_local_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx & bytes4(0x11223344))); }
    function t_retdirect_and() external view  returns (uint256) { return uint256(uint32(_s_retdirect_and() & bytes4(0x11223344))); }
    function t_ret_and() external view  returns (uint256) { bytes4 rx = _s_ret_and(); return uint256(uint32(rx & bytes4(0x11223344))); }
    function t_param_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_and(rx); }
    function t_copy_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_ternary_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_ifjoin_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_loopjoin_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_tuple_and() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_modifier_and() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_and(rx); }
    function t_ptr_and() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_and; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_mem_and() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry & bytes4(0x11223344))); }
    function t_local_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx | bytes4(0x11223344))); }
    function t_retdirect_or() external view  returns (uint256) { return uint256(uint32(_s_retdirect_or() | bytes4(0x11223344))); }
    function t_ret_or() external view  returns (uint256) { bytes4 rx = _s_ret_or(); return uint256(uint32(rx | bytes4(0x11223344))); }
    function t_param_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_or(rx); }
    function t_copy_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_ternary_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_ifjoin_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_loopjoin_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_tuple_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_modifier_or() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_or(rx); }
    function t_ptr_or() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_or; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_mem_or() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry | bytes4(0x11223344))); }
    function t_local_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx ^ bytes4(0x11223344))); }
    function t_retdirect_xor() external view  returns (uint256) { return uint256(uint32(_s_retdirect_xor() ^ bytes4(0x11223344))); }
    function t_ret_xor() external view  returns (uint256) { bytes4 rx = _s_ret_xor(); return uint256(uint32(rx ^ bytes4(0x11223344))); }
    function t_param_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_xor(rx); }
    function t_copy_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_ternary_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_ifjoin_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_loopjoin_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_tuple_xor() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_modifier_xor() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_xor(rx); }
    function t_ptr_xor() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_xor; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_mem_xor() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry ^ bytes4(0x11223344))); }
    function t_local_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(~rx)); }
    function t_retdirect_not() external view  returns (uint256) { return uint256(uint32(~_s_retdirect_not())); }
    function t_ret_not() external view  returns (uint256) { bytes4 rx = _s_ret_not(); return uint256(uint32(~rx)); }
    function t_param_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_not(rx); }
}
contract Cb4_1 {
    uint256 _res_modifier_not;
    modifier _m_modifier_not(bytes4 pv) { _res_modifier_not = uint256(uint32(~pv)); _; }
    function _e_modifier_not(bytes4 rd) internal _m_modifier_not(rd)  returns (uint256) { return _res_modifier_not; }
    function _w_ptr_not() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmpd_or() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmpd_or() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmpd_or(bytes4 pv) internal view  returns (uint256) { bytes4 z = bytes4(0x11223344); z |= pv; return uint256(uint32(z)); }
    uint256 _res_modifier_cmpd_or;
    modifier _m_modifier_cmpd_or(bytes4 pv) { bytes4 z = bytes4(0x11223344); z |= pv; _res_modifier_cmpd_or = uint256(uint32(z)); _; }
    function _e_modifier_cmpd_or(bytes4 rd) internal _m_modifier_cmpd_or(rd)  returns (uint256) { return _res_modifier_cmpd_or; }
    function _w_ptr_cmpd_or() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_copy_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(~ry)); }
    function t_ternary_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(~ry)); }
    function t_ifjoin_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(~ry)); }
    function t_loopjoin_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(~ry)); }
    function t_tuple_not() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(~ry)); }
    function t_modifier_not() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_not(rx); }
    function t_ptr_not() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_not; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(~ry)); }
    function t_mem_not() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(~ry)); }
    function t_local_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 z = bytes4(0x11223344); z |= rx; return uint256(uint32(z)); }
    function t_retdirect_cmpd_or() external view  returns (uint256) { bytes4 z = bytes4(0x11223344); z |= _s_retdirect_cmpd_or(); return uint256(uint32(z)); }
    function t_ret_cmpd_or() external view  returns (uint256) { bytes4 rx = _s_ret_cmpd_or(); bytes4 z = bytes4(0x11223344); z |= rx; return uint256(uint32(z)); }
    function t_param_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmpd_or(rx); }
    function t_copy_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_ternary_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_ifjoin_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_loopjoin_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_tuple_cmpd_or() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_modifier_cmpd_or() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmpd_or(rx); }
    function t_ptr_cmpd_or() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmpd_or; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
    function t_mem_cmpd_or() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 z = bytes4(0x11223344); z |= ry; return uint256(uint32(z)); }
}
contract Cb20_0 {
    function _s_retdirect_and() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_and(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv & bytes20(0x1111111111111111111111111111111111111111))); }
    function _s_retdirect_or() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_or(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv | bytes20(0x1111111111111111111111111111111111111111))); }
    function _s_retdirect_xor() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_xor(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv ^ bytes20(0x1111111111111111111111111111111111111111))); }
    function _s_retdirect_not() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_not(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(~pv)); }
    function _s_retdirect_cmpd_or() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmpd_or(bytes20 pv) internal view  returns (uint256) { bytes20 z = bytes20(0x1111111111111111111111111111111111111111); z |= pv; return uint256(uint160(z)); }
    function t_local_and() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx & bytes20(0x1111111111111111111111111111111111111111))); }
    function t_retdirect_and() external view  returns (uint256) { return uint256(uint160(_s_retdirect_and() & bytes20(0x1111111111111111111111111111111111111111))); }
    function t_param_and() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_and(rx); }
    function t_mem_and() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry & bytes20(0x1111111111111111111111111111111111111111))); }
    function t_local_or() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx | bytes20(0x1111111111111111111111111111111111111111))); }
    function t_retdirect_or() external view  returns (uint256) { return uint256(uint160(_s_retdirect_or() | bytes20(0x1111111111111111111111111111111111111111))); }
    function t_param_or() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_or(rx); }
    function t_mem_or() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry | bytes20(0x1111111111111111111111111111111111111111))); }
    function t_local_xor() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx ^ bytes20(0x1111111111111111111111111111111111111111))); }
    function t_retdirect_xor() external view  returns (uint256) { return uint256(uint160(_s_retdirect_xor() ^ bytes20(0x1111111111111111111111111111111111111111))); }
    function t_param_xor() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_xor(rx); }
    function t_mem_xor() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry ^ bytes20(0x1111111111111111111111111111111111111111))); }
    function t_local_not() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(~rx)); }
    function t_retdirect_not() external view  returns (uint256) { return uint256(uint160(~_s_retdirect_not())); }
    function t_param_not() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_not(rx); }
    function t_mem_not() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(~ry)); }
    function t_local_cmpd_or() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 z = bytes20(0x1111111111111111111111111111111111111111); z |= rx; return uint256(uint160(z)); }
    function t_retdirect_cmpd_or() external view  returns (uint256) { bytes20 z = bytes20(0x1111111111111111111111111111111111111111); z |= _s_retdirect_cmpd_or(); return uint256(uint160(z)); }
    function t_param_cmpd_or() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmpd_or(rx); }
    function t_mem_cmpd_or() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 z = bytes20(0x1111111111111111111111111111111111111111); z |= ry; return uint256(uint160(z)); }
}
