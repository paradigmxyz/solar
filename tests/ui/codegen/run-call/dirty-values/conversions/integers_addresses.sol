//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_ret_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_param_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_copy_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_ternary_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_ifjoin_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_loopjoin_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_tuple_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_modifier_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_ptr_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Caddr_0::t_local_to_uint8 => 144
//@ run-call: Caddr_0::t_retdirect_to_uint8 => 144
//@ run-call: Caddr_0::t_ret_to_uint8 => 144
//@ run-call: Caddr_0::t_param_to_uint8 => 144
//@ run-call: Caddr_0::t_copy_to_uint8 => 144
//@ run-call: Caddr_0::t_ternary_to_uint8 => 144
//@ run-call: Caddr_0::t_ifjoin_to_uint8 => 144
//@ run-call: Caddr_0::t_loopjoin_to_uint8 => 144
//@ run-call: Caddr_0::t_tuple_to_uint8 => 144
//@ run-call: Caddr_0::t_modifier_to_uint8 => 144
//@ run-call: Caddr_0::t_ptr_to_uint8 => 144
//@ run-call: Caddr_0::t_mem_to_uint8 => 144
//@ run-call: Caddrp_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_param_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Caddrp_0::t_local_to_uint8 => 144
//@ run-call: Caddrp_0::t_retdirect_to_uint8 => 144
//@ run-call: Caddrp_0::t_param_to_uint8 => 144
//@ run-call: Caddrp_0::t_mem_to_uint8 => 144
//@ run-call: Cctr_0::t_local_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_uint256 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_retdirect_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_param_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_mem_to_uint64 => 6230888492328974480
//@ run-call: Cctr_0::t_local_to_uint8 => 144
//@ run-call: Cctr_0::t_retdirect_to_uint8 => 144
//@ run-call: Cctr_0::t_param_to_uint8 => 144
//@ run-call: Cctr_0::t_mem_to_uint8 => 144

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

contract Caddr_0 {
    function _s_retdirect_to_uint256() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint256() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(address pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    uint256 _res_modifier_to_uint256;
    modifier _m_modifier_to_uint256(address pv) { _res_modifier_to_uint256 = uint256(uint160(address(pv))); _; }
    function _e_modifier_to_uint256(address rd) internal _m_modifier_to_uint256(rd)  returns (uint256) { return _res_modifier_to_uint256; }
    function _w_ptr_to_uint256() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_uint64() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint64() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(address pv) internal view  returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    uint256 _res_modifier_to_uint64;
    modifier _m_modifier_to_uint64(address pv) { _res_modifier_to_uint64 = uint256(uint64(uint160(address(pv)))); _; }
    function _e_modifier_to_uint64(address rd) internal _m_modifier_to_uint64(rd)  returns (uint256) { return _res_modifier_to_uint64; }
    function _w_ptr_to_uint64() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_uint8() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint8() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(address pv) internal view  returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    uint256 _res_modifier_to_uint8;
    modifier _m_modifier_to_uint8(address pv) { _res_modifier_to_uint8 = uint256(uint8(uint160(address(pv)))); _; }
    function _e_modifier_to_uint8(address rd) internal _m_modifier_to_uint8(rd)  returns (uint256) { return _res_modifier_to_uint8; }
    function _w_ptr_to_uint8() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_ret_to_uint256() external view  returns (uint256) { address rx = _s_ret_to_uint256(); return uint256(uint160(address(rx))); }
    function t_param_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_copy_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(ry))); }
    function t_ternary_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(ry))); }
    function t_ifjoin_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(ry))); }
    function t_loopjoin_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(ry))); }
    function t_tuple_to_uint256() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(ry))); }
    function t_modifier_to_uint256() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint256(rx); }
    function t_ptr_to_uint256() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint256; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(ry))); }
    function t_mem_to_uint256() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view  returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_ret_to_uint64() external view  returns (uint256) { address rx = _s_ret_to_uint64(); return uint256(uint64(uint160(address(rx)))); }
    function t_param_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_copy_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint64(uint160(address(ry)))); }
    function t_ternary_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint64(uint160(address(ry)))); }
    function t_ifjoin_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint64(uint160(address(ry)))); }
    function t_loopjoin_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint64(uint160(address(ry)))); }
    function t_tuple_to_uint64() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint64(uint160(address(ry)))); }
    function t_modifier_to_uint64() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint64(rx); }
    function t_ptr_to_uint64() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint64; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint64(uint160(address(ry)))); }
    function t_mem_to_uint64() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view  returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_ret_to_uint8() external view  returns (uint256) { address rx = _s_ret_to_uint8(); return uint256(uint8(uint160(address(rx)))); }
    function t_param_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_copy_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint8(uint160(address(ry)))); }
    function t_ternary_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint8(uint160(address(ry)))); }
    function t_ifjoin_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(uint160(address(ry)))); }
    function t_loopjoin_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(uint160(address(ry)))); }
    function t_tuple_to_uint8() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(uint160(address(ry)))); }
    function t_modifier_to_uint8() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint8(rx); }
    function t_ptr_to_uint8() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint8; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint8(uint160(address(ry)))); }
    function t_mem_to_uint8() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
}
contract Caddrp_0 {
    function _s_retdirect_to_uint256() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(address payable pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_uint64() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(address payable pv) internal view  returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    function _s_retdirect_to_uint8() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(address payable pv) internal view  returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    function t_local_to_uint256() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_param_to_uint256() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint64() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view  returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_param_to_uint64() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_mem_to_uint64() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view  returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_param_to_uint8() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
}
contract Cctr_0 {
    function _s_retdirect_to_uint256() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint256(IC pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_uint64() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint64(IC pv) internal view  returns (uint256) { return uint256(uint64(uint160(address(pv)))); }
    function _s_retdirect_to_uint8() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint8(IC pv) internal view  returns (uint256) { return uint256(uint8(uint160(address(pv)))); }
    function t_local_to_uint256() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint256() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint256()))); }
    function t_param_to_uint256() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint256(rx); }
    function t_mem_to_uint256() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_uint64() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint64(uint160(address(rx)))); }
    function t_retdirect_to_uint64() external view  returns (uint256) { return uint256(uint64(uint160(address(_s_retdirect_to_uint64())))); }
    function t_param_to_uint64() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint64(rx); }
    function t_mem_to_uint64() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint64(uint160(address(ry)))); }
    function t_local_to_uint8() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint8(uint160(address(rx)))); }
    function t_retdirect_to_uint8() external view  returns (uint256) { return uint256(uint8(uint160(address(_s_retdirect_to_uint8())))); }
    function t_param_to_uint8() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint8(rx); }
    function t_mem_to_uint8() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint8(uint160(address(ry)))); }
}
