//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_to_bytes => 27
//@ run-call: Cu8_0::t_retdirect_to_bytes => 27
//@ run-call: Cu8_0::t_ret_to_bytes => 27
//@ run-call: Cu8_0::t_param_to_bytes => 27
//@ run-call: Cu8_0::t_copy_to_bytes => 27
//@ run-call: Cu8_0::t_ternary_to_bytes => 27
//@ run-call: Cu8_0::t_ifjoin_to_bytes => 27
//@ run-call: Cu8_0::t_loopjoin_to_bytes => 27
//@ run-call: Cu8_0::t_tuple_to_bytes => 27
//@ run-call: Cu8_0::t_modifier_to_bytes => 27
//@ run-call: Cu8_0::t_ptr_to_bytes => 27
//@ run-call: Cu8_0::t_mem_to_bytes => 27
//@ run-call: Cu64_0::t_local_to_bytes => 27
//@ run-call: Cu64_0::t_retdirect_to_bytes => 27
//@ run-call: Cu64_0::t_param_to_bytes => 27
//@ run-call: Cu64_0::t_mem_to_bytes => 27
//@ run-call: Caddr_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_bytes20 => 103929005307130220006098923584552504982110632080
//@ run-call: Cb1_0::t_local_to_bytes2 => 43776
//@ run-call: Cb1_0::t_retdirect_to_bytes2 => 43776
//@ run-call: Cb1_0::t_param_to_bytes2 => 43776
//@ run-call: Cb1_0::t_mem_to_bytes2 => 43776
//@ run-call: Cb1_0::t_local_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_retdirect_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_param_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_mem_to_bytes4 => 2868903936
//@ run-call: Cb1_0::t_local_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_retdirect_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_param_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_mem_to_bytes8 => 12321848580485677056
//@ run-call: Cb1_0::t_local_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_retdirect_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_param_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_mem_to_bytes16 => 227297987279220614266551007307938922496
//@ run-call: Cb1_0::t_local_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_retdirect_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_param_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_mem_to_bytes20 => 976237421810876558643867603103454673285798690816
//@ run-call: Cb1_0::t_local_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_to_bytes32 => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_to_uint => 171
//@ run-call: Cb1_0::t_retdirect_to_uint => 171
//@ run-call: Cb1_0::t_param_to_uint => 171
//@ run-call: Cb1_0::t_mem_to_uint => 171
//@ run-call: Cb1_0::t_local_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_retdirect_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_param_to_bytes_shift_back => 171
//@ run-call: Cb1_0::t_mem_to_bytes_shift_back => 171
//@ run-call: Cb4_0::t_local_to_bytes1 => 222
//@ run-call: Cb4_0::t_retdirect_to_bytes1 => 222
//@ run-call: Cb4_0::t_ret_to_bytes1 => 222
//@ run-call: Cb4_0::t_param_to_bytes1 => 222
//@ run-call: Cb4_0::t_copy_to_bytes1 => 222
//@ run-call: Cb4_0::t_ternary_to_bytes1 => 222
//@ run-call: Cb4_0::t_ifjoin_to_bytes1 => 222
//@ run-call: Cb4_0::t_loopjoin_to_bytes1 => 222
//@ run-call: Cb4_0::t_tuple_to_bytes1 => 222
//@ run-call: Cb4_0::t_modifier_to_bytes1 => 222
//@ run-call: Cb4_0::t_ptr_to_bytes1 => 222
//@ run-call: Cb4_0::t_mem_to_bytes1 => 222
//@ run-call: Cb4_0::t_local_to_bytes2 => 57005
//@ run-call: Cb4_0::t_retdirect_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ret_to_bytes2 => 57005
//@ run-call: Cb4_0::t_param_to_bytes2 => 57005
//@ run-call: Cb4_0::t_copy_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ternary_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ifjoin_to_bytes2 => 57005
//@ run-call: Cb4_0::t_loopjoin_to_bytes2 => 57005
//@ run-call: Cb4_0::t_tuple_to_bytes2 => 57005
//@ run-call: Cb4_0::t_modifier_to_bytes2 => 57005
//@ run-call: Cb4_0::t_ptr_to_bytes2 => 57005
//@ run-call: Cb4_0::t_mem_to_bytes2 => 57005
//@ run-call: Cb4_0::t_local_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_retdirect_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ret_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_param_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_copy_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ternary_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ifjoin_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_loopjoin_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_tuple_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_modifier_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_ptr_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_mem_to_bytes8 => 16045690981097406464
//@ run-call: Cb4_0::t_local_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_retdirect_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_ret_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_0::t_param_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_copy_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ternary_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ifjoin_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_loopjoin_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_tuple_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_modifier_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_ptr_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_mem_to_bytes16 => 295990755014133383690938178081940045824
//@ run-call: Cb4_1::t_local_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_retdirect_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ret_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_param_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_copy_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ternary_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ifjoin_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_loopjoin_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_tuple_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_modifier_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_ptr_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_mem_to_bytes20 => 1271270612704050900734399246419756505046821371904
//@ run-call: Cb4_1::t_local_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_retdirect_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ret_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_param_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_copy_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ternary_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ifjoin_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_loopjoin_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_tuple_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_modifier_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_ptr_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_mem_to_bytes32 => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_1::t_local_to_uint => 3735928559
//@ run-call: Cb4_1::t_retdirect_to_uint => 3735928559
//@ run-call: Cb4_1::t_ret_to_uint => 3735928559
//@ run-call: Cb4_1::t_param_to_uint => 3735928559
//@ run-call: Cb4_1::t_copy_to_uint => 3735928559
//@ run-call: Cb4_1::t_ternary_to_uint => 3735928559
//@ run-call: Cb4_1::t_ifjoin_to_uint => 3735928559
//@ run-call: Cb4_1::t_loopjoin_to_uint => 3735928559
//@ run-call: Cb4_2::t_tuple_to_uint => 3735928559
//@ run-call: Cb4_2::t_modifier_to_uint => 3735928559
//@ run-call: Cb4_2::t_ptr_to_uint => 3735928559
//@ run-call: Cb4_2::t_mem_to_uint => 3735928559
//@ run-call: Cb4_2::t_local_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_retdirect_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ret_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_param_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_copy_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ternary_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ifjoin_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_loopjoin_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_tuple_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_modifier_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_ptr_to_bytes_shift_back => 3735928559
//@ run-call: Cb4_2::t_mem_to_bytes_shift_back => 3735928559
//@ run-call: Cb20_0::t_local_to_bytes1 => 18
//@ run-call: Cb20_0::t_retdirect_to_bytes1 => 18
//@ run-call: Cb20_0::t_param_to_bytes1 => 18
//@ run-call: Cb20_0::t_mem_to_bytes1 => 18
//@ run-call: Cb20_0::t_local_to_bytes2 => 4660
//@ run-call: Cb20_0::t_retdirect_to_bytes2 => 4660
//@ run-call: Cb20_0::t_param_to_bytes2 => 4660
//@ run-call: Cb20_0::t_mem_to_bytes2 => 4660
//@ run-call: Cb20_0::t_local_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_retdirect_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_param_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_mem_to_bytes4 => 305419896
//@ run-call: Cb20_0::t_local_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_retdirect_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_param_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_mem_to_bytes8 => 1311768467284833366
//@ run-call: Cb20_0::t_local_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_retdirect_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_param_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_mem_to_bytes16 => 24197857199965561741520400062332047378
//@ run-call: Cb20_0::t_local_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_to_bytes32 => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_uint => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_bytes_shift_back => 103929005307130220006098923584552504982110632080
//@ run-call: Cud_0::t_local_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_retdirect_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_param_unwrap_to_bytes => 27
//@ run-call: Cud_0::t_mem_unwrap_to_bytes => 27

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
    function _s_retdirect_to_bytes() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_bytes() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_bytes(uint8 pv) internal view  returns (uint256) { return uint256(uint8(bytes1(pv))); }
    uint256 _res_modifier_to_bytes;
    modifier _m_modifier_to_bytes(uint8 pv) { _res_modifier_to_bytes = uint256(uint8(bytes1(pv))); _; }
    function _e_modifier_to_bytes(uint8 rd) internal _m_modifier_to_bytes(rd)  returns (uint256) { return _res_modifier_to_bytes; }
    function _w_ptr_to_bytes() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes() external view  returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes()))); }
    function t_ret_to_bytes() external view  returns (uint256) { uint8 rx = _s_ret_to_bytes(); return uint256(uint8(bytes1(rx))); }
    function t_param_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_bytes(rx); }
    function t_copy_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint8(bytes1(ry))); }
    function t_ternary_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint8(bytes1(ry))); }
    function t_ifjoin_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(bytes1(ry))); }
    function t_loopjoin_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(bytes1(ry))); }
    function t_tuple_to_bytes() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(bytes1(ry))); }
    function t_modifier_to_bytes() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_bytes(rx); }
    function t_ptr_to_bytes() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint8(bytes1(ry))); }
    function t_mem_to_bytes() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
}
contract Cu64_0 {
    function _s_retdirect_to_bytes() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_bytes(uint64 pv) internal view  returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function t_local_to_bytes() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes() external view  returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes()))); }
    function t_param_to_bytes() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_bytes(rx); }
    function t_mem_to_bytes() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
}
contract Caddr_0 {
    function _s_retdirect_to_bytes20() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_bytes20() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(address pv) internal view  returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    uint256 _res_modifier_to_bytes20;
    modifier _m_modifier_to_bytes20(address pv) { _res_modifier_to_bytes20 = uint256(uint160(bytes20(address(pv)))); _; }
    function _e_modifier_to_bytes20(address rd) internal _m_modifier_to_bytes20(rd)  returns (uint256) { return _res_modifier_to_bytes20; }
    function _w_ptr_to_bytes20() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view  returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_ret_to_bytes20() external view  returns (uint256) { address rx = _s_ret_to_bytes20(); return uint256(uint160(bytes20(address(rx)))); }
    function t_param_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_copy_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(bytes20(address(ry)))); }
    function t_ternary_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(bytes20(address(ry)))); }
    function t_ifjoin_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(bytes20(address(ry)))); }
    function t_loopjoin_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(bytes20(address(ry)))); }
    function t_tuple_to_bytes20() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(bytes20(address(ry)))); }
    function t_modifier_to_bytes20() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_bytes20(rx); }
    function t_ptr_to_bytes20() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes20; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(bytes20(address(ry)))); }
    function t_mem_to_bytes20() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
}
contract Caddrp_0 {
    function _s_retdirect_to_bytes20() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(address payable pv) internal view  returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    function t_local_to_bytes20() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view  returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_param_to_bytes20() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
}
contract Cctr_0 {
    function _s_retdirect_to_bytes20() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_bytes20(IC pv) internal view  returns (uint256) { return uint256(uint160(bytes20(address(pv)))); }
    function t_local_to_bytes20() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(bytes20(address(rx)))); }
    function t_retdirect_to_bytes20() external view  returns (uint256) { return uint256(uint160(bytes20(address(_s_retdirect_to_bytes20())))); }
    function t_param_to_bytes20() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(bytes20(address(ry)))); }
}
contract Cb1_0 {
    function _s_retdirect_to_bytes2() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes2(bytes1 pv) internal view  returns (uint256) { return uint256(uint16(bytes2(pv))); }
    function _s_retdirect_to_bytes4() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes4(bytes1 pv) internal view  returns (uint256) { return uint256(uint32(bytes4(pv))); }
    function _s_retdirect_to_bytes8() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes8(bytes1 pv) internal view  returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function _s_retdirect_to_bytes16() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes16(bytes1 pv) internal view  returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function _s_retdirect_to_bytes20() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes20(bytes1 pv) internal view  returns (uint256) { return uint256(uint160(bytes20(pv))); }
    function _s_retdirect_to_bytes32() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes32(bytes1 pv) internal view  returns (uint256) { return uint256(uint256(bytes32(pv))); }
    function _s_retdirect_to_uint() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_uint(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_to_bytes_shift_back(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(bytes1(uint8(pv)))); }
    function t_local_to_bytes2() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view  returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_param_to_bytes2() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes2(rx); }
    function t_mem_to_bytes2() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes4() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint32(bytes4(rx))); }
    function t_retdirect_to_bytes4() external view  returns (uint256) { return uint256(uint32(bytes4(_s_retdirect_to_bytes4()))); }
    function t_param_to_bytes4() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes4(rx); }
    function t_mem_to_bytes4() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint32(bytes4(ry))); }
    function t_local_to_bytes8() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view  returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_param_to_bytes8() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes8(rx); }
    function t_mem_to_bytes8() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view  returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_param_to_bytes16() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes16(rx); }
    function t_mem_to_bytes16() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes20() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint160(bytes20(rx))); }
    function t_retdirect_to_bytes20() external view  returns (uint256) { return uint256(uint160(bytes20(_s_retdirect_to_bytes20()))); }
    function t_param_to_bytes20() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes20(rx); }
    function t_mem_to_bytes20() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint160(bytes20(ry))); }
    function t_local_to_bytes32() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view  returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_param_to_bytes32() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes32(rx); }
    function t_mem_to_bytes32() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx)); }
    function t_retdirect_to_uint() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint())); }
    function t_param_to_uint() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_uint(rx); }
    function t_mem_to_uint() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry)); }
    function t_local_to_bytes_shift_back() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(bytes1(uint8(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view  returns (uint256) { return uint256(uint8(bytes1(uint8(_s_retdirect_to_bytes_shift_back())))); }
    function t_param_to_bytes_shift_back() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_to_bytes_shift_back(rx); }
    function t_mem_to_bytes_shift_back() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(bytes1(uint8(ry)))); }
}
contract Cb4_0 {
    function _s_retdirect_to_bytes1() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes1() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes1(bytes4 pv) internal view  returns (uint256) { return uint256(uint8(bytes1(pv))); }
    uint256 _res_modifier_to_bytes1;
    modifier _m_modifier_to_bytes1(bytes4 pv) { _res_modifier_to_bytes1 = uint256(uint8(bytes1(pv))); _; }
    function _e_modifier_to_bytes1(bytes4 rd) internal _m_modifier_to_bytes1(rd)  returns (uint256) { return _res_modifier_to_bytes1; }
    function _w_ptr_to_bytes1() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes2() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes2() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes2(bytes4 pv) internal view  returns (uint256) { return uint256(uint16(bytes2(pv))); }
    uint256 _res_modifier_to_bytes2;
    modifier _m_modifier_to_bytes2(bytes4 pv) { _res_modifier_to_bytes2 = uint256(uint16(bytes2(pv))); _; }
    function _e_modifier_to_bytes2(bytes4 rd) internal _m_modifier_to_bytes2(rd)  returns (uint256) { return _res_modifier_to_bytes2; }
    function _w_ptr_to_bytes2() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes8() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes8() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes8(bytes4 pv) internal view  returns (uint256) { return uint256(uint64(bytes8(pv))); }
    uint256 _res_modifier_to_bytes8;
    modifier _m_modifier_to_bytes8(bytes4 pv) { _res_modifier_to_bytes8 = uint256(uint64(bytes8(pv))); _; }
    function _e_modifier_to_bytes8(bytes4 rd) internal _m_modifier_to_bytes8(rd)  returns (uint256) { return _res_modifier_to_bytes8; }
    function _w_ptr_to_bytes8() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes16() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes16() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes16(bytes4 pv) internal view  returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function t_local_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes1() external view  returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes1()))); }
    function t_ret_to_bytes1() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes1(); return uint256(uint8(bytes1(rx))); }
    function t_param_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes1(rx); }
    function t_copy_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint8(bytes1(ry))); }
    function t_ternary_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint8(bytes1(ry))); }
    function t_ifjoin_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(bytes1(ry))); }
    function t_loopjoin_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(bytes1(ry))); }
    function t_tuple_to_bytes1() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(bytes1(ry))); }
    function t_modifier_to_bytes1() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes1(rx); }
    function t_ptr_to_bytes1() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes1; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint8(bytes1(ry))); }
    function t_mem_to_bytes1() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
    function t_local_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view  returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_ret_to_bytes2() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes2(); return uint256(uint16(bytes2(rx))); }
    function t_param_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes2(rx); }
    function t_copy_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint16(bytes2(ry))); }
    function t_ternary_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint16(bytes2(ry))); }
    function t_ifjoin_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint16(bytes2(ry))); }
    function t_loopjoin_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint16(bytes2(ry))); }
    function t_tuple_to_bytes2() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint16(bytes2(ry))); }
    function t_modifier_to_bytes2() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes2(rx); }
    function t_ptr_to_bytes2() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes2; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint16(bytes2(ry))); }
    function t_mem_to_bytes2() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view  returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_ret_to_bytes8() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes8(); return uint256(uint64(bytes8(rx))); }
    function t_param_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes8(rx); }
    function t_copy_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint64(bytes8(ry))); }
    function t_ternary_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint64(bytes8(ry))); }
    function t_ifjoin_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(bytes8(ry))); }
    function t_loopjoin_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(bytes8(ry))); }
    function t_tuple_to_bytes8() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(bytes8(ry))); }
    function t_modifier_to_bytes8() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes8(rx); }
    function t_ptr_to_bytes8() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes8; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint64(bytes8(ry))); }
    function t_mem_to_bytes8() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view  returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_ret_to_bytes16() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes16(); return uint256(uint128(bytes16(rx))); }
    function t_param_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes16(rx); }
}
contract Cb4_1 {
    uint256 _res_modifier_to_bytes16;
    modifier _m_modifier_to_bytes16(bytes4 pv) { _res_modifier_to_bytes16 = uint256(uint128(bytes16(pv))); _; }
    function _e_modifier_to_bytes16(bytes4 rd) internal _m_modifier_to_bytes16(rd)  returns (uint256) { return _res_modifier_to_bytes16; }
    function _w_ptr_to_bytes16() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes20() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes20() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes20(bytes4 pv) internal view  returns (uint256) { return uint256(uint160(bytes20(pv))); }
    uint256 _res_modifier_to_bytes20;
    modifier _m_modifier_to_bytes20(bytes4 pv) { _res_modifier_to_bytes20 = uint256(uint160(bytes20(pv))); _; }
    function _e_modifier_to_bytes20(bytes4 rd) internal _m_modifier_to_bytes20(rd)  returns (uint256) { return _res_modifier_to_bytes20; }
    function _w_ptr_to_bytes20() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes32() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes32() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes32(bytes4 pv) internal view  returns (uint256) { return uint256(uint256(bytes32(pv))); }
    uint256 _res_modifier_to_bytes32;
    modifier _m_modifier_to_bytes32(bytes4 pv) { _res_modifier_to_bytes32 = uint256(uint256(bytes32(pv))); _; }
    function _e_modifier_to_bytes32(bytes4 rd) internal _m_modifier_to_bytes32(rd)  returns (uint256) { return _res_modifier_to_bytes32; }
    function _w_ptr_to_bytes32() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_uint() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_uint() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_uint(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv)); }
    function t_copy_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint128(bytes16(ry))); }
    function t_ternary_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint128(bytes16(ry))); }
    function t_ifjoin_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint128(bytes16(ry))); }
    function t_loopjoin_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint128(bytes16(ry))); }
    function t_tuple_to_bytes16() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint128(bytes16(ry))); }
    function t_modifier_to_bytes16() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes16(rx); }
    function t_ptr_to_bytes16() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes16; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint128(bytes16(ry))); }
    function t_mem_to_bytes16() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint160(bytes20(rx))); }
    function t_retdirect_to_bytes20() external view  returns (uint256) { return uint256(uint160(bytes20(_s_retdirect_to_bytes20()))); }
    function t_ret_to_bytes20() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes20(); return uint256(uint160(bytes20(rx))); }
    function t_param_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes20(rx); }
    function t_copy_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint160(bytes20(ry))); }
    function t_ternary_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint160(bytes20(ry))); }
    function t_ifjoin_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(bytes20(ry))); }
    function t_loopjoin_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(bytes20(ry))); }
    function t_tuple_to_bytes20() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(bytes20(ry))); }
    function t_modifier_to_bytes20() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes20(rx); }
    function t_ptr_to_bytes20() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes20; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint160(bytes20(ry))); }
    function t_mem_to_bytes20() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint160(bytes20(ry))); }
    function t_local_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view  returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_ret_to_bytes32() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes32(); return uint256(uint256(bytes32(rx))); }
    function t_param_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes32(rx); }
    function t_copy_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint256(bytes32(ry))); }
    function t_ternary_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint256(bytes32(ry))); }
    function t_ifjoin_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint256(bytes32(ry))); }
    function t_loopjoin_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint256(bytes32(ry))); }
    function t_tuple_to_bytes32() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint256(bytes32(ry))); }
    function t_modifier_to_bytes32() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes32(rx); }
    function t_ptr_to_bytes32() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes32; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint256(bytes32(ry))); }
    function t_mem_to_bytes32() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx)); }
    function t_retdirect_to_uint() external view  returns (uint256) { return uint256(uint32(_s_retdirect_to_uint())); }
    function t_ret_to_uint() external view  returns (uint256) { bytes4 rx = _s_ret_to_uint(); return uint256(uint32(rx)); }
    function t_param_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_uint(rx); }
    function t_copy_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry)); }
    function t_ternary_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry)); }
    function t_ifjoin_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry)); }
    function t_loopjoin_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry)); }
}
contract Cb4_2 {
    uint256 _res_modifier_to_uint;
    modifier _m_modifier_to_uint(bytes4 pv) { _res_modifier_to_uint = uint256(uint32(pv)); _; }
    function _e_modifier_to_uint(bytes4 rd) internal _m_modifier_to_uint(rd)  returns (uint256) { return _res_modifier_to_uint; }
    function _w_ptr_to_uint() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_to_bytes_shift_back() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_to_bytes_shift_back(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(bytes4(uint32(pv)))); }
    uint256 _res_modifier_to_bytes_shift_back;
    modifier _m_modifier_to_bytes_shift_back(bytes4 pv) { _res_modifier_to_bytes_shift_back = uint256(uint32(bytes4(uint32(pv)))); _; }
    function _e_modifier_to_bytes_shift_back(bytes4 rd) internal _m_modifier_to_bytes_shift_back(rd)  returns (uint256) { return _res_modifier_to_bytes_shift_back; }
    function _w_ptr_to_bytes_shift_back() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_tuple_to_uint() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry)); }
    function t_modifier_to_uint() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_uint(rx); }
    function t_ptr_to_uint() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry)); }
    function t_mem_to_uint() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry)); }
    function t_local_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(bytes4(uint32(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view  returns (uint256) { return uint256(uint32(bytes4(uint32(_s_retdirect_to_bytes_shift_back())))); }
    function t_ret_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx = _s_ret_to_bytes_shift_back(); return uint256(uint32(bytes4(uint32(rx)))); }
    function t_param_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_to_bytes_shift_back(rx); }
    function t_copy_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_ternary_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_ifjoin_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(bytes4(uint32(ry)))); }
    function t_loopjoin_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(bytes4(uint32(ry)))); }
    function t_tuple_to_bytes_shift_back() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(bytes4(uint32(ry)))); }
    function t_modifier_to_bytes_shift_back() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_to_bytes_shift_back(rx); }
    function t_ptr_to_bytes_shift_back() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_bytes_shift_back; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(bytes4(uint32(ry)))); }
    function t_mem_to_bytes_shift_back() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(bytes4(uint32(ry)))); }
}
contract Cb20_0 {
    function _s_retdirect_to_bytes1() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes1(bytes20 pv) internal view  returns (uint256) { return uint256(uint8(bytes1(pv))); }
    function _s_retdirect_to_bytes2() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes2(bytes20 pv) internal view  returns (uint256) { return uint256(uint16(bytes2(pv))); }
    function _s_retdirect_to_bytes4() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes4(bytes20 pv) internal view  returns (uint256) { return uint256(uint32(bytes4(pv))); }
    function _s_retdirect_to_bytes8() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes8(bytes20 pv) internal view  returns (uint256) { return uint256(uint64(bytes8(pv))); }
    function _s_retdirect_to_bytes16() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes16(bytes20 pv) internal view  returns (uint256) { return uint256(uint128(bytes16(pv))); }
    function _s_retdirect_to_bytes32() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes32(bytes20 pv) internal view  returns (uint256) { return uint256(uint256(bytes32(pv))); }
    function _s_retdirect_to_uint() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_uint(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv)); }
    function _s_retdirect_to_bytes_shift_back() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_bytes_shift_back(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(bytes20(uint160(pv)))); }
    function t_local_to_bytes1() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint8(bytes1(rx))); }
    function t_retdirect_to_bytes1() external view  returns (uint256) { return uint256(uint8(bytes1(_s_retdirect_to_bytes1()))); }
    function t_param_to_bytes1() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes1(rx); }
    function t_mem_to_bytes1() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint8(bytes1(ry))); }
    function t_local_to_bytes2() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint16(bytes2(rx))); }
    function t_retdirect_to_bytes2() external view  returns (uint256) { return uint256(uint16(bytes2(_s_retdirect_to_bytes2()))); }
    function t_param_to_bytes2() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes2(rx); }
    function t_mem_to_bytes2() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint16(bytes2(ry))); }
    function t_local_to_bytes4() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint32(bytes4(rx))); }
    function t_retdirect_to_bytes4() external view  returns (uint256) { return uint256(uint32(bytes4(_s_retdirect_to_bytes4()))); }
    function t_param_to_bytes4() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes4(rx); }
    function t_mem_to_bytes4() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint32(bytes4(ry))); }
    function t_local_to_bytes8() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint64(bytes8(rx))); }
    function t_retdirect_to_bytes8() external view  returns (uint256) { return uint256(uint64(bytes8(_s_retdirect_to_bytes8()))); }
    function t_param_to_bytes8() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes8(rx); }
    function t_mem_to_bytes8() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint64(bytes8(ry))); }
    function t_local_to_bytes16() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint128(bytes16(rx))); }
    function t_retdirect_to_bytes16() external view  returns (uint256) { return uint256(uint128(bytes16(_s_retdirect_to_bytes16()))); }
    function t_param_to_bytes16() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes16(rx); }
    function t_mem_to_bytes16() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint128(bytes16(ry))); }
    function t_local_to_bytes32() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint256(bytes32(rx))); }
    function t_retdirect_to_bytes32() external view  returns (uint256) { return uint256(uint256(bytes32(_s_retdirect_to_bytes32()))); }
    function t_param_to_bytes32() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes32(rx); }
    function t_mem_to_bytes32() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint256(bytes32(ry))); }
    function t_local_to_uint() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx)); }
    function t_retdirect_to_uint() external view  returns (uint256) { return uint256(uint160(_s_retdirect_to_uint())); }
    function t_param_to_uint() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_uint(rx); }
    function t_mem_to_uint() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry)); }
    function t_local_to_bytes_shift_back() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(bytes20(uint160(rx)))); }
    function t_retdirect_to_bytes_shift_back() external view  returns (uint256) { return uint256(uint160(bytes20(uint160(_s_retdirect_to_bytes_shift_back())))); }
    function t_param_to_bytes_shift_back() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_bytes_shift_back(rx); }
    function t_mem_to_bytes_shift_back() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(bytes20(uint160(ry)))); }
}
contract Cud_0 {
    function _s_retdirect_unwrap_to_bytes() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap_to_bytes(U pv) internal view  returns (uint256) { return uint256(uint8(bytes1(U.unwrap(pv)))); }
    function t_local_unwrap_to_bytes() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(uint8(bytes1(U.unwrap(rx)))); }
    function t_retdirect_unwrap_to_bytes() external view  returns (uint256) { return uint256(uint8(bytes1(U.unwrap(_s_retdirect_unwrap_to_bytes())))); }
    function t_param_unwrap_to_bytes() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap_to_bytes(rx); }
    function t_mem_unwrap_to_bytes() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(uint8(bytes1(U.unwrap(ry)))); }
}
