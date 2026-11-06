//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_retdirect_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_ret_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_param_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_copy_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_ternary_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_ifjoin_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_loopjoin_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_tuple_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_modifier_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_ptr_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu8_0::t_mem_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cu64_0::t_local_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_retdirect_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_ret_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_param_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_copy_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_ternary_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_ifjoin_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_loopjoin_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_tuple_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_modifier_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_ptr_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Cu64_0::t_mem_ripemd_packed => 828849227236904459152106566334604131632963767879
//@ run-call: Ci8_0::t_local_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_retdirect_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_ret_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_param_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_copy_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_ternary_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_ifjoin_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_loopjoin_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_tuple_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_modifier_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_ptr_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci8_0::t_mem_ripemd_packed => 1314443666375572958916505419820004802941365240782
//@ run-call: Ci24_0::t_local_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_retdirect_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_ret_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_param_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_copy_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_ternary_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_ifjoin_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_loopjoin_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_tuple_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_modifier_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_ptr_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Ci24_0::t_mem_ripemd_packed => 1324389150155752813468542199978670315781416718375
//@ run-call: Caddr_0::t_local_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_retdirect_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_ret_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_param_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_copy_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_ternary_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_ifjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_loopjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_tuple_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_modifier_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_ptr_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddr_0::t_mem_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_local_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_retdirect_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_ret_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_param_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_copy_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_ternary_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_ifjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_loopjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_tuple_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_modifier_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_ptr_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Caddrp_0::t_mem_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_local_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_retdirect_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_ret_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_param_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_copy_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_ternary_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_ifjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_loopjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_tuple_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_modifier_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_ptr_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cctr_0::t_mem_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cbool_0::t_local_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_retdirect_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_ret_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_param_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_copy_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_ternary_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_ifjoin_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_loopjoin_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_tuple_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_modifier_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_ptr_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cbool_0::t_mem_ripemd_packed => 1384825604730001502228850657425354160336156615091
//@ run-call: Cb1_0::t_local_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_retdirect_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_ret_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_param_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_copy_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_ternary_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_ifjoin_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_loopjoin_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_tuple_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_modifier_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_ptr_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb1_0::t_mem_ripemd_packed => 745232240533215831957475978649097276734172086763
//@ run-call: Cb4_0::t_local_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_retdirect_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_ret_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_param_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_copy_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_ternary_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_ifjoin_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_loopjoin_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_tuple_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_modifier_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_ptr_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb4_0::t_mem_ripemd_packed => 196427904754877468875886265012084216901481155418
//@ run-call: Cb20_0::t_local_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_retdirect_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_ret_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_param_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_copy_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_ternary_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_ifjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_loopjoin_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_tuple_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_modifier_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_ptr_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call: Cb20_0::t_mem_ripemd_packed => 195396709473492461476699212479943479183946711203
//@ run-call-fail: Cen_0::t_local_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ripemd_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_retdirect_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_ret_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_param_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_copy_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_ternary_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_ifjoin_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_loopjoin_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_tuple_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_modifier_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_ptr_ripemd_packed => 302114531880322253742388250343013087560145343832
//@ run-call: Cud_0::t_mem_ripemd_packed => 302114531880322253742388250343013087560145343832

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: builtins with value parameters.

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
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_ripemd_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ripemd_packed(uint8 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(uint8 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(uint8 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { uint8 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cu64_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_ripemd_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ripemd_packed(uint64 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(uint64 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(uint64 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function t_local_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { uint64 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Ci8_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _s_ret_ripemd_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ripemd_packed(int8 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(int8 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(int8 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcdfd; }
    function t_local_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { int8 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } bool rf = gasleft() > 0; int8 ro = int8(-2); int8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 ry = int8(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } (int8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (int8) rq; assembly { rq := rp } int8 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Ci24_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _s_ret_ripemd_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ripemd_packed(int24 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(int24 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(int24 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcdfffffd; }
    function t_local_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { int24 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } bool rf = gasleft() > 0; int24 ro = int24(-2); int24 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 ry = int24(-2); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } (int24 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (int24) rq; assembly { rq := rp } int24 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Caddr_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_ripemd_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ripemd_packed(address pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(address pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(address rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { address rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Caddrp_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_ripemd_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ripemd_packed(address payable pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(address payable pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(address payable rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { address payable rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address payable ro = payable(address(0x1111111111111111111111111111111111111111)); address payable ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable ry = payable(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address payable ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (address payable) rq; assembly { rq := rp } address payable ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cctr_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_ripemd_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ripemd_packed(IC pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(IC pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(IC rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { IC rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; IC ro = IC(address(0x1111111111111111111111111111111111111111)); IC ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC ry = IC(address(0x1111111111111111111111111111111111111111)); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (IC ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (IC) rq; assembly { rq := rp } IC ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cbool_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _s_ret_ripemd_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ripemd_packed(bool pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(bool pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(bool rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0x100; }
    function t_local_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { bool rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = false; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } (bool ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (bool) rq; assembly { rq := rp } bool ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cb1_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _s_ret_ripemd_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ripemd_packed(bytes1 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(bytes1 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(bytes1 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xab0000000000000000000000000000000000000000000000000000000000ffff; }
    function t_local_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { bytes1 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bool rf = gasleft() > 0; bytes1 ro = bytes1(0xcd); bytes1 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 ry = bytes1(0xcd); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } (bytes1 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (bytes1) rq; assembly { rq := rp } bytes1 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cb4_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_ripemd_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ripemd_packed(bytes4 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(bytes4 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(bytes4 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_local_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { bytes4 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cb20_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _s_ret_ripemd_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ripemd_packed(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(bytes20 pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(bytes20 rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0x123456789012345678901234567890123456789000000000000000000000abcd; }
    function t_local_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { bytes20 rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bool rf = gasleft() > 0; bytes20 ro = bytes20(0x1111111111111111111111111111111111111111); bytes20 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 ry = bytes20(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } (bytes20 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (bytes20) rq; assembly { rq := rp } bytes20 ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cen_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_ripemd_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ripemd_packed(E pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(E pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(E rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0x101; }
    function t_local_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { E rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
contract Cud_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_ripemd_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _s_ret_ripemd_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ripemd_packed(U pv) internal view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(pv)))); }
    uint256 _res_modifier_ripemd_packed;
    modifier _m_modifier_ripemd_packed(U pv) { _res_modifier_ripemd_packed = uint256(uint160(ripemd160(abi.encodePacked(pv)))); _; }
    function _e_modifier_ripemd_packed(U rd) internal _m_modifier_ripemd_packed(rd)  returns (uint256) { return _res_modifier_ripemd_packed; }
    function _w_ptr_ripemd_packed() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_retdirect_ripemd_packed() external view  returns (uint256) { return uint256(uint160(ripemd160(abi.encodePacked(_s_retdirect_ripemd_packed())))); }
    function t_ret_ripemd_packed() external view  returns (uint256) { U rx = _s_ret_ripemd_packed(); return uint256(uint160(ripemd160(abi.encodePacked(rx)))); }
    function t_param_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ripemd_packed(rx); }
    function t_copy_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = rx; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ternary_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry = rf ? rx : ro; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_ifjoin_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_loopjoin_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = U.wrap(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_tuple_ripemd_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } (U ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_modifier_ripemd_packed() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _e_modifier_ripemd_packed(rx); }
    function t_ptr_ripemd_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ripemd_packed; function() internal pure returns (U) rq; assembly { rq := rp } U ry = rq(); return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
    function t_mem_ripemd_packed() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(uint160(ripemd160(abi.encodePacked(ry)))); }
}
