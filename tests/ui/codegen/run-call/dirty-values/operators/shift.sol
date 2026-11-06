//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_retdirect_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_ret_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_param_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_copy_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_ternary_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_ifjoin_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_loopjoin_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_tuple_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_modifier_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_ptr_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_mem_shift_lhs_all => 7168380497920595351160446172966549915730777584558944950131228231094311043244
//@ run-call: Cu8_0::t_local_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_retdirect_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_ret_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_param_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_copy_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_ternary_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_ifjoin_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_loopjoin_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_tuple_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_modifier_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_ptr_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_mem_shift_rhs_all => 108933827041705421341480571288029503889706897316271602902184988844985554815643
//@ run-call: Cu8_0::t_local_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_retdirect_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_ret_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_param_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_copy_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_ternary_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_ifjoin_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_loopjoin_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_tuple_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_modifier_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_ptr_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu8_0::t_mem_compound_shift => 78338746147236970124700731725183845421594913511827187288591969170390706184117
//@ run-call: Cu64_0::t_local_shift_lhs_all => 67043942510783131965895509910367793334234493457039039190643514795724575915574
//@ run-call: Cu64_0::t_retdirect_shift_lhs_all => 67043942510783131965895509910367793334234493457039039190643514795724575915574
//@ run-call: Cu64_0::t_param_shift_lhs_all => 67043942510783131965895509910367793334234493457039039190643514795724575915574
//@ run-call: Cu64_0::t_mem_shift_lhs_all => 67043942510783131965895509910367793334234493457039039190643514795724575915574
//@ run-call: Cu64_0::t_local_shift_rhs_all => 112571820662518430692830609231936548898833785438089971133328457084137109934457
//@ run-call: Cu64_0::t_retdirect_shift_rhs_all => 112571820662518430692830609231936548898833785438089971133328457084137109934457
//@ run-call: Cu64_0::t_param_shift_rhs_all => 112571820662518430692830609231936548898833785438089971133328457084137109934457
//@ run-call: Cu64_0::t_mem_shift_rhs_all => 112571820662518430692830609231936548898833785438089971133328457084137109934457
//@ run-call: Cu64_0::t_local_compound_shift => 76178275142042256443470156784410759391886618300244326598323303913832786219387
//@ run-call: Cu64_0::t_retdirect_compound_shift => 76178275142042256443470156784410759391886618300244326598323303913832786219387
//@ run-call: Cu64_0::t_param_compound_shift => 76178275142042256443470156784410759391886618300244326598323303913832786219387
//@ run-call: Cu64_0::t_mem_compound_shift => 76178275142042256443470156784410759391886618300244326598323303913832786219387
//@ run-call: Ci8_0::t_local_shift_lhs_all => 48741082954873483368035135862293673378808874780937003881239363009509442022531
//@ run-call: Ci8_0::t_retdirect_shift_lhs_all => 48741082954873483368035135862293673378808874780937003881239363009509442022531
//@ run-call: Ci8_0::t_param_shift_lhs_all => 48741082954873483368035135862293673378808874780937003881239363009509442022531
//@ run-call: Ci8_0::t_mem_shift_lhs_all => 48741082954873483368035135862293673378808874780937003881239363009509442022531
//@ run-call: Ci24_0::t_local_shift_lhs_all => 58887228197345353566054020210716931840671137277420375246845291298826962614154
//@ run-call: Ci24_0::t_retdirect_shift_lhs_all => 58887228197345353566054020210716931840671137277420375246845291298826962614154
//@ run-call: Ci24_0::t_param_shift_lhs_all => 58887228197345353566054020210716931840671137277420375246845291298826962614154
//@ run-call: Ci24_0::t_mem_shift_lhs_all => 58887228197345353566054020210716931840671137277420375246845291298826962614154
//@ run-call: Cb1_0::t_local_shl => 0
//@ run-call: Cb1_0::t_retdirect_shl => 0
//@ run-call: Cb1_0::t_param_shl => 0
//@ run-call: Cb1_0::t_mem_shl => 0
//@ run-call: Cb1_0::t_local_shr => 0
//@ run-call: Cb1_0::t_retdirect_shr => 0
//@ run-call: Cb1_0::t_param_shr => 0
//@ run-call: Cb1_0::t_mem_shr => 0
//@ run-call: Cb1_0::t_local_shl_var => 88
//@ run-call: Cb1_0::t_retdirect_shl_var => 88
//@ run-call: Cb1_0::t_param_shl_var => 88
//@ run-call: Cb1_0::t_mem_shl_var => 88
//@ run-call: Cb1_0::t_local_cmpd_shl => 0
//@ run-call: Cb1_0::t_retdirect_cmpd_shl => 0
//@ run-call: Cb1_0::t_param_cmpd_shl => 0
//@ run-call: Cb1_0::t_mem_cmpd_shl => 0
//@ run-call: Cb4_0::t_local_shl => 2914971392
//@ run-call: Cb4_0::t_retdirect_shl => 2914971392
//@ run-call: Cb4_0::t_ret_shl => 2914971392
//@ run-call: Cb4_0::t_param_shl => 2914971392
//@ run-call: Cb4_0::t_copy_shl => 2914971392
//@ run-call: Cb4_0::t_ternary_shl => 2914971392
//@ run-call: Cb4_0::t_ifjoin_shl => 2914971392
//@ run-call: Cb4_0::t_loopjoin_shl => 2914971392
//@ run-call: Cb4_0::t_tuple_shl => 2914971392
//@ run-call: Cb4_0::t_modifier_shl => 2914971392
//@ run-call: Cb4_0::t_ptr_shl => 2914971392
//@ run-call: Cb4_0::t_mem_shl => 2914971392
//@ run-call: Cb4_0::t_local_shr => 14593470
//@ run-call: Cb4_0::t_retdirect_shr => 14593470
//@ run-call: Cb4_0::t_ret_shr => 14593470
//@ run-call: Cb4_0::t_param_shr => 14593470
//@ run-call: Cb4_0::t_copy_shr => 14593470
//@ run-call: Cb4_0::t_ternary_shr => 14593470
//@ run-call: Cb4_0::t_ifjoin_shr => 14593470
//@ run-call: Cb4_0::t_loopjoin_shr => 14593470
//@ run-call: Cb4_0::t_tuple_shr => 14593470
//@ run-call: Cb4_0::t_modifier_shr => 14593470
//@ run-call: Cb4_0::t_ptr_shr => 14593470
//@ run-call: Cb4_0::t_mem_shr => 14593470
//@ run-call: Cb4_0::t_local_shl_var => 4117624696
//@ run-call: Cb4_0::t_retdirect_shl_var => 4117624696
//@ run-call: Cb4_0::t_ret_shl_var => 4117624696
//@ run-call: Cb4_0::t_param_shl_var => 4117624696
//@ run-call: Cb4_0::t_copy_shl_var => 4117624696
//@ run-call: Cb4_0::t_ternary_shl_var => 4117624696
//@ run-call: Cb4_0::t_ifjoin_shl_var => 4117624696
//@ run-call: Cb4_0::t_loopjoin_shl_var => 4117624696
//@ run-call: Cb4_0::t_tuple_shl_var => 4117624696
//@ run-call: Cb4_0::t_modifier_shl_var => 4117624696
//@ run-call: Cb4_0::t_ptr_shl_var => 4117624696
//@ run-call: Cb4_0::t_mem_shl_var => 4117624696
//@ run-call: Cb4_0::t_local_cmpd_shl => 2914971392
//@ run-call: Cb4_0::t_retdirect_cmpd_shl => 2914971392
//@ run-call: Cb4_0::t_ret_cmpd_shl => 2914971392
//@ run-call: Cb4_0::t_param_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_copy_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_ternary_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_ifjoin_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_loopjoin_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_tuple_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_modifier_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_ptr_cmpd_shl => 2914971392
//@ run-call: Cb4_1::t_mem_cmpd_shl => 2914971392
//@ run-call: Cb20_0::t_local_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_retdirect_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_param_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_mem_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_local_shr => 405972676980977421898823920252158222586369656
//@ run-call: Cb20_0::t_retdirect_shr => 405972676980977421898823920252158222586369656
//@ run-call: Cb20_0::t_param_shr => 405972676980977421898823920252158222586369656
//@ run-call: Cb20_0::t_mem_shr => 405972676980977421898823920252158222586369656
//@ run-call: Cb20_0::t_local_shl_var => 831432042457041760048791388676420039856885056640
//@ run-call: Cb20_0::t_retdirect_shl_var => 831432042457041760048791388676420039856885056640
//@ run-call: Cb20_0::t_param_shl_var => 831432042457041760048791388676420039856885056640
//@ run-call: Cb20_0::t_mem_shl_var => 831432042457041760048791388676420039856885056640
//@ run-call: Cb20_0::t_local_cmpd_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_retdirect_cmpd_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_param_cmpd_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cb20_0::t_mem_cmpd_shl => 298795886669083793894997448752346921613536038912
//@ run-call: Cud_0::t_local_shl => 54
//@ run-call: Cud_0::t_retdirect_shl => 54
//@ run-call: Cud_0::t_param_shl => 54
//@ run-call: Cud_0::t_mem_shl => 54

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
    function _s_retdirect_shift_lhs_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_shift_lhs_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_shift_lhs_all(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv << 1, pv >> 1, pv << 9, pv >> 9))); }
    uint256 _res_modifier_shift_lhs_all;
    modifier _m_modifier_shift_lhs_all(uint8 pv) { _res_modifier_shift_lhs_all = uint256(keccak256(abi.encode(pv << 1, pv >> 1, pv << 9, pv >> 9))); _; }
    function _e_modifier_shift_lhs_all(uint8 rd) internal _m_modifier_shift_lhs_all(rd)  returns (uint256) { return _res_modifier_shift_lhs_all; }
    function _w_ptr_shift_lhs_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_shift_rhs_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_shift_rhs_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_shift_rhs_all(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(uint256(1) << pv, (uint256(1) << 200) >> pv, uint8(3) << pv, uint8(3) >> pv))); }
    uint256 _res_modifier_shift_rhs_all;
    modifier _m_modifier_shift_rhs_all(uint8 pv) { _res_modifier_shift_rhs_all = uint256(keccak256(abi.encode(uint256(1) << pv, (uint256(1) << 200) >> pv, uint8(3) << pv, uint8(3) >> pv))); _; }
    function _e_modifier_shift_rhs_all(uint8 rd) internal _m_modifier_shift_rhs_all(rd)  returns (uint256) { return _res_modifier_shift_rhs_all; }
    function _w_ptr_shift_rhs_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_compound_shift() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_compound_shift() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_compound_shift(uint8 pv) internal view  returns (uint256) { uint8 z1 = uint8(3); z1 <<= pv; uint8 z2 = uint8(3); z2 >>= pv; return uint256(keccak256(abi.encode(z1, z2))); }
    uint256 _res_modifier_compound_shift;
    modifier _m_modifier_compound_shift(uint8 pv) { uint8 z1 = uint8(3); z1 <<= pv; uint8 z2 = uint8(3); z2 >>= pv; _res_modifier_compound_shift = uint256(keccak256(abi.encode(z1, z2))); _; }
    function _e_modifier_compound_shift(uint8 rd) internal _m_modifier_compound_shift(rd)  returns (uint256) { return _res_modifier_compound_shift; }
    function _w_ptr_compound_shift() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx << 1, rx >> 1, rx << 9, rx >> 9))); }
    function t_retdirect_shift_lhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_shift_lhs_all() << 1, _s_retdirect_shift_lhs_all() >> 1, _s_retdirect_shift_lhs_all() << 9, _s_retdirect_shift_lhs_all() >> 9))); }
    function t_ret_shift_lhs_all() external view  returns (uint256) { uint8 rx = _s_ret_shift_lhs_all(); return uint256(keccak256(abi.encode(rx << 1, rx >> 1, rx << 9, rx >> 9))); }
    function t_param_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_shift_lhs_all(rx); }
    function t_copy_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_ternary_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_ifjoin_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_loopjoin_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_tuple_shift_lhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_modifier_shift_lhs_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_shift_lhs_all(rx); }
    function t_ptr_shift_lhs_all() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_shift_lhs_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_mem_shift_lhs_all() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_local_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(uint256(1) << rx, (uint256(1) << 200) >> rx, uint8(3) << rx, uint8(3) >> rx))); }
    function t_retdirect_shift_rhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(uint256(1) << _s_retdirect_shift_rhs_all(), (uint256(1) << 200) >> _s_retdirect_shift_rhs_all(), uint8(3) << _s_retdirect_shift_rhs_all(), uint8(3) >> _s_retdirect_shift_rhs_all()))); }
    function t_ret_shift_rhs_all() external view  returns (uint256) { uint8 rx = _s_ret_shift_rhs_all(); return uint256(keccak256(abi.encode(uint256(1) << rx, (uint256(1) << 200) >> rx, uint8(3) << rx, uint8(3) >> rx))); }
    function t_param_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_shift_rhs_all(rx); }
    function t_copy_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_ternary_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_ifjoin_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_loopjoin_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_tuple_shift_rhs_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_modifier_shift_rhs_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_shift_rhs_all(rx); }
    function t_ptr_shift_rhs_all() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_shift_rhs_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_mem_shift_rhs_all() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint8(3) << ry, uint8(3) >> ry))); }
    function t_local_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 z1 = uint8(3); z1 <<= rx; uint8 z2 = uint8(3); z2 >>= rx; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_compound_shift() external view  returns (uint256) { uint8 z1 = uint8(3); z1 <<= _s_retdirect_compound_shift(); uint8 z2 = uint8(3); z2 >>= _s_retdirect_compound_shift(); return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ret_compound_shift() external view  returns (uint256) { uint8 rx = _s_ret_compound_shift(); uint8 z1 = uint8(3); z1 <<= rx; uint8 z2 = uint8(3); z2 >>= rx; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_compound_shift(rx); }
    function t_copy_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ternary_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_ifjoin_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_loopjoin_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_tuple_compound_shift() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_modifier_compound_shift() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_compound_shift(rx); }
    function t_ptr_compound_shift() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_compound_shift; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_mem_compound_shift() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 z1 = uint8(3); z1 <<= ry; uint8 z2 = uint8(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Cu64_0 {
    function _s_retdirect_shift_lhs_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_shift_lhs_all(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv << 1, pv >> 1, pv << 9, pv >> 9))); }
    function _s_retdirect_shift_rhs_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_shift_rhs_all(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(uint256(1) << pv, (uint256(1) << 200) >> pv, uint64(3) << pv, uint64(3) >> pv))); }
    function _s_retdirect_compound_shift() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_compound_shift(uint64 pv) internal view  returns (uint256) { uint64 z1 = uint64(3); z1 <<= pv; uint64 z2 = uint64(3); z2 >>= pv; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_local_shift_lhs_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(rx << 1, rx >> 1, rx << 9, rx >> 9))); }
    function t_retdirect_shift_lhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_shift_lhs_all() << 1, _s_retdirect_shift_lhs_all() >> 1, _s_retdirect_shift_lhs_all() << 9, _s_retdirect_shift_lhs_all() >> 9))); }
    function t_param_shift_lhs_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_shift_lhs_all(rx); }
    function t_mem_shift_lhs_all() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
    function t_local_shift_rhs_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(uint256(1) << rx, (uint256(1) << 200) >> rx, uint64(3) << rx, uint64(3) >> rx))); }
    function t_retdirect_shift_rhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(uint256(1) << _s_retdirect_shift_rhs_all(), (uint256(1) << 200) >> _s_retdirect_shift_rhs_all(), uint64(3) << _s_retdirect_shift_rhs_all(), uint64(3) >> _s_retdirect_shift_rhs_all()))); }
    function t_param_shift_rhs_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_shift_rhs_all(rx); }
    function t_mem_shift_rhs_all() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(uint256(1) << ry, (uint256(1) << 200) >> ry, uint64(3) << ry, uint64(3) >> ry))); }
    function t_local_compound_shift() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 z1 = uint64(3); z1 <<= rx; uint64 z2 = uint64(3); z2 >>= rx; return uint256(keccak256(abi.encode(z1, z2))); }
    function t_retdirect_compound_shift() external view  returns (uint256) { uint64 z1 = uint64(3); z1 <<= _s_retdirect_compound_shift(); uint64 z2 = uint64(3); z2 >>= _s_retdirect_compound_shift(); return uint256(keccak256(abi.encode(z1, z2))); }
    function t_param_compound_shift() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_compound_shift(rx); }
    function t_mem_compound_shift() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 z1 = uint64(3); z1 <<= ry; uint64 z2 = uint64(3); z2 >>= ry; return uint256(keccak256(abi.encode(z1, z2))); }
}
contract Ci8_0 {
    function _s_retdirect_shift_lhs_all() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_shift_lhs_all(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv << 1, pv >> 1, pv << 9, pv >> 9))); }
    function t_local_shift_lhs_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(rx << 1, rx >> 1, rx << 9, rx >> 9))); }
    function t_retdirect_shift_lhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_shift_lhs_all() << 1, _s_retdirect_shift_lhs_all() >> 1, _s_retdirect_shift_lhs_all() << 9, _s_retdirect_shift_lhs_all() >> 9))); }
    function t_param_shift_lhs_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_shift_lhs_all(rx); }
    function t_mem_shift_lhs_all() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
}
contract Ci24_0 {
    function _s_retdirect_shift_lhs_all() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_shift_lhs_all(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv << 1, pv >> 1, pv << 9, pv >> 9))); }
    function t_local_shift_lhs_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(rx << 1, rx >> 1, rx << 9, rx >> 9))); }
    function t_retdirect_shift_lhs_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_shift_lhs_all() << 1, _s_retdirect_shift_lhs_all() >> 1, _s_retdirect_shift_lhs_all() << 9, _s_retdirect_shift_lhs_all() >> 9))); }
    function t_param_shift_lhs_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_shift_lhs_all(rx); }
    function t_mem_shift_lhs_all() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(ry << 1, ry >> 1, ry << 9, ry >> 9))); }
}
contract Cb1_0 {
    function _s_retdirect_shl() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_shl(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv << 8)); }
    function _s_retdirect_shr() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_shr(bytes1 pv) internal view  returns (uint256) { return uint256(uint8(pv >> 8)); }
    function _s_retdirect_shl_var() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_shl_var(bytes1 pv) internal view  returns (uint256) { uint256 n = 3; return uint256(uint8(pv << (n))); }
    function _s_retdirect_cmpd_shl() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_cmpd_shl(bytes1 pv) internal view  returns (uint256) { bytes1 z = pv; z <<= 8; return uint256(uint8(z)); }
    function t_local_shl() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx << 8)); }
    function t_retdirect_shl() external view  returns (uint256) { return uint256(uint8(_s_retdirect_shl() << 8)); }
    function t_param_shl() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_shl(rx); }
    function t_mem_shl() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry << 8)); }
    function t_local_shr() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint8(rx >> 8)); }
    function t_retdirect_shr() external view  returns (uint256) { return uint256(uint8(_s_retdirect_shr() >> 8)); }
    function t_param_shr() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_shr(rx); }
    function t_mem_shr() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint8(ry >> 8)); }
    function t_local_shl_var() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } uint256 n = 3; return uint256(uint8(rx << (n))); }
    function t_retdirect_shl_var() external view  returns (uint256) { uint256 n = 3; return uint256(uint8(_s_retdirect_shl_var() << (n))); }
    function t_param_shl_var() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_shl_var(rx); }
    function t_mem_shl_var() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; uint256 n = 3; return uint256(uint8(ry << (n))); }
    function t_local_cmpd_shl() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 z = rx; z <<= 8; return uint256(uint8(z)); }
    function t_retdirect_cmpd_shl() external view  returns (uint256) { bytes1 z = _s_retdirect_cmpd_shl(); z <<= 8; return uint256(uint8(z)); }
    function t_param_cmpd_shl() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_cmpd_shl(rx); }
    function t_mem_cmpd_shl() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 z = ry; z <<= 8; return uint256(uint8(z)); }
}
contract Cb4_0 {
    function _s_retdirect_shl() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_shl() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_shl(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv << 8)); }
    uint256 _res_modifier_shl;
    modifier _m_modifier_shl(bytes4 pv) { _res_modifier_shl = uint256(uint32(pv << 8)); _; }
    function _e_modifier_shl(bytes4 rd) internal _m_modifier_shl(rd)  returns (uint256) { return _res_modifier_shl; }
    function _w_ptr_shl() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_shr() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_shr() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_shr(bytes4 pv) internal view  returns (uint256) { return uint256(uint32(pv >> 8)); }
    uint256 _res_modifier_shr;
    modifier _m_modifier_shr(bytes4 pv) { _res_modifier_shr = uint256(uint32(pv >> 8)); _; }
    function _e_modifier_shr(bytes4 rd) internal _m_modifier_shr(rd)  returns (uint256) { return _res_modifier_shr; }
    function _w_ptr_shr() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_shl_var() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_shl_var() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_shl_var(bytes4 pv) internal view  returns (uint256) { uint256 n = 3; return uint256(uint32(pv << (n))); }
    uint256 _res_modifier_shl_var;
    modifier _m_modifier_shl_var(bytes4 pv) { uint256 n = 3; _res_modifier_shl_var = uint256(uint32(pv << (n))); _; }
    function _e_modifier_shl_var(bytes4 rd) internal _m_modifier_shl_var(rd)  returns (uint256) { return _res_modifier_shl_var; }
    function _w_ptr_shl_var() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function _s_retdirect_cmpd_shl() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_cmpd_shl() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_cmpd_shl(bytes4 pv) internal view  returns (uint256) { bytes4 z = pv; z <<= 8; return uint256(uint32(z)); }
    function t_local_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx << 8)); }
    function t_retdirect_shl() external view  returns (uint256) { return uint256(uint32(_s_retdirect_shl() << 8)); }
    function t_ret_shl() external view  returns (uint256) { bytes4 rx = _s_ret_shl(); return uint256(uint32(rx << 8)); }
    function t_param_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_shl(rx); }
    function t_copy_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry << 8)); }
    function t_ternary_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry << 8)); }
    function t_ifjoin_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry << 8)); }
    function t_loopjoin_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry << 8)); }
    function t_tuple_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry << 8)); }
    function t_modifier_shl() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_shl(rx); }
    function t_ptr_shl() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_shl; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry << 8)); }
    function t_mem_shl() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry << 8)); }
    function t_local_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint32(rx >> 8)); }
    function t_retdirect_shr() external view  returns (uint256) { return uint256(uint32(_s_retdirect_shr() >> 8)); }
    function t_ret_shr() external view  returns (uint256) { bytes4 rx = _s_ret_shr(); return uint256(uint32(rx >> 8)); }
    function t_param_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_shr(rx); }
    function t_copy_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint32(ry >> 8)); }
    function t_ternary_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint32(ry >> 8)); }
    function t_ifjoin_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint32(ry >> 8)); }
    function t_loopjoin_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint32(ry >> 8)); }
    function t_tuple_shr() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint32(ry >> 8)); }
    function t_modifier_shr() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_shr(rx); }
    function t_ptr_shr() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_shr; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint32(ry >> 8)); }
    function t_mem_shr() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint32(ry >> 8)); }
    function t_local_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } uint256 n = 3; return uint256(uint32(rx << (n))); }
    function t_retdirect_shl_var() external view  returns (uint256) { uint256 n = 3; return uint256(uint32(_s_retdirect_shl_var() << (n))); }
    function t_ret_shl_var() external view  returns (uint256) { bytes4 rx = _s_ret_shl_var(); uint256 n = 3; return uint256(uint32(rx << (n))); }
    function t_param_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_shl_var(rx); }
    function t_copy_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_ternary_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_ifjoin_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_loopjoin_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_tuple_shl_var() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_modifier_shl_var() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_shl_var(rx); }
    function t_ptr_shl_var() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_shl_var; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_mem_shl_var() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; uint256 n = 3; return uint256(uint32(ry << (n))); }
    function t_local_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 z = rx; z <<= 8; return uint256(uint32(z)); }
    function t_retdirect_cmpd_shl() external view  returns (uint256) { bytes4 z = _s_retdirect_cmpd_shl(); z <<= 8; return uint256(uint32(z)); }
    function t_ret_cmpd_shl() external view  returns (uint256) { bytes4 rx = _s_ret_cmpd_shl(); bytes4 z = rx; z <<= 8; return uint256(uint32(z)); }
    function t_param_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_cmpd_shl(rx); }
}
contract Cb4_1 {
    uint256 _res_modifier_cmpd_shl;
    modifier _m_modifier_cmpd_shl(bytes4 pv) { bytes4 z = pv; z <<= 8; _res_modifier_cmpd_shl = uint256(uint32(z)); _; }
    function _e_modifier_cmpd_shl(bytes4 rd) internal _m_modifier_cmpd_shl(rd)  returns (uint256) { return _res_modifier_cmpd_shl; }
    function _w_ptr_cmpd_shl() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_copy_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_ternary_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_ifjoin_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_loopjoin_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_tuple_cmpd_shl() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_modifier_cmpd_shl() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_cmpd_shl(rx); }
    function t_ptr_cmpd_shl() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmpd_shl; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
    function t_mem_cmpd_shl() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 z = ry; z <<= 8; return uint256(uint32(z)); }
}
contract Cb20_0 {
    function _s_retdirect_shl() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_shl(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv << 8)); }
    function _s_retdirect_shr() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_shr(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(pv >> 8)); }
    function _s_retdirect_shl_var() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_shl_var(bytes20 pv) internal view  returns (uint256) { uint256 n = 3; return uint256(uint160(pv << (n))); }
    function _s_retdirect_cmpd_shl() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_cmpd_shl(bytes20 pv) internal view  returns (uint256) { bytes20 z = pv; z <<= 8; return uint256(uint160(z)); }
    function t_local_shl() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx << 8)); }
    function t_retdirect_shl() external view  returns (uint256) { return uint256(uint160(_s_retdirect_shl() << 8)); }
    function t_param_shl() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_shl(rx); }
    function t_mem_shl() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry << 8)); }
    function t_local_shr() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(rx >> 8)); }
    function t_retdirect_shr() external view  returns (uint256) { return uint256(uint160(_s_retdirect_shr() >> 8)); }
    function t_param_shr() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_shr(rx); }
    function t_mem_shr() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ry >> 8)); }
    function t_local_shl_var() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } uint256 n = 3; return uint256(uint160(rx << (n))); }
    function t_retdirect_shl_var() external view  returns (uint256) { uint256 n = 3; return uint256(uint160(_s_retdirect_shl_var() << (n))); }
    function t_param_shl_var() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_shl_var(rx); }
    function t_mem_shl_var() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; uint256 n = 3; return uint256(uint160(ry << (n))); }
    function t_local_cmpd_shl() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 z = rx; z <<= 8; return uint256(uint160(z)); }
    function t_retdirect_cmpd_shl() external view  returns (uint256) { bytes20 z = _s_retdirect_cmpd_shl(); z <<= 8; return uint256(uint160(z)); }
    function t_param_cmpd_shl() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_cmpd_shl(rx); }
    function t_mem_cmpd_shl() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 z = ry; z <<= 8; return uint256(uint160(z)); }
}
contract Cud_0 {
    function _s_retdirect_shl() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_shl(U pv) internal view  returns (uint256) { return uint256(U.unwrap(pv) << 1); }
    function t_local_shl() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(rx) << 1); }
    function t_retdirect_shl() external view  returns (uint256) { return uint256(U.unwrap(_s_retdirect_shl()) << 1); }
    function t_param_shl() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_shl(rx); }
    function t_mem_shl() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(ry) << 1); }
}
