//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_to_address => 27
//@ run-call: Cu8_0::t_retdirect_to_address => 27
//@ run-call: Cu8_0::t_ret_to_address => 27
//@ run-call: Cu8_0::t_param_to_address => 27
//@ run-call: Cu8_0::t_copy_to_address => 27
//@ run-call: Cu8_0::t_ternary_to_address => 27
//@ run-call: Cu8_0::t_ifjoin_to_address => 27
//@ run-call: Cu8_0::t_loopjoin_to_address => 27
//@ run-call: Cu8_0::t_tuple_to_address => 27
//@ run-call: Cu8_0::t_modifier_to_address => 27
//@ run-call: Cu8_0::t_ptr_to_address => 27
//@ run-call: Cu8_0::t_mem_to_address => 27
//@ run-call: Cu64_0::t_local_to_address => 27
//@ run-call: Cu64_0::t_retdirect_to_address => 27
//@ run-call: Cu64_0::t_param_to_address => 27
//@ run-call: Cu64_0::t_mem_to_address => 27
//@ run-call: Caddr_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ret_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_copy_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ternary_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ifjoin_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_loopjoin_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_modifier_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_ptr_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_to_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_uint160 => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_address_payable => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_to_ic_addr => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_to_address => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_to_address => 103929005307130220006098923584552504982110632080

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
    function _s_retdirect_to_address() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_to_address() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_to_address(uint8 pv) internal view  returns (uint256) { return uint256(uint160(address(uint160(pv)))); }
    uint256 _res_modifier_to_address;
    modifier _m_modifier_to_address(uint8 pv) { _res_modifier_to_address = uint256(uint160(address(uint160(pv)))); _; }
    function _e_modifier_to_address(uint8 rd) internal _m_modifier_to_address(rd)  returns (uint256) { return _res_modifier_to_address; }
    function _w_ptr_to_address() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint160(address(uint160(rx)))); }
    function t_retdirect_to_address() external view  returns (uint256) { return uint256(uint160(address(uint160(_s_retdirect_to_address())))); }
    function t_ret_to_address() external view  returns (uint256) { uint8 rx = _s_ret_to_address(); return uint256(uint160(address(uint160(rx)))); }
    function t_param_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_to_address(rx); }
    function t_copy_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint160(address(uint160(ry)))); }
    function t_ternary_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint160(address(uint160(ry)))); }
    function t_ifjoin_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(uint160(ry)))); }
    function t_loopjoin_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(uint160(ry)))); }
    function t_tuple_to_address() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(uint160(ry)))); }
    function t_modifier_to_address() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_to_address(rx); }
    function t_ptr_to_address() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_address; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint160(address(uint160(ry)))); }
    function t_mem_to_address() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint160(address(uint160(ry)))); }
}
contract Cu64_0 {
    function _s_retdirect_to_address() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_to_address(uint64 pv) internal view  returns (uint256) { return uint256(uint160(address(uint160(pv)))); }
    function t_local_to_address() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint160(address(uint160(rx)))); }
    function t_retdirect_to_address() external view  returns (uint256) { return uint256(uint160(address(uint160(_s_retdirect_to_address())))); }
    function t_param_to_address() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_to_address(rx); }
    function t_mem_to_address() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint160(address(uint160(ry)))); }
}
contract Caddr_0 {
    function _s_retdirect_to_uint160() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_uint160() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(address pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    uint256 _res_modifier_to_uint160;
    modifier _m_modifier_to_uint160(address pv) { _res_modifier_to_uint160 = uint256(uint160(address(pv))); _; }
    function _e_modifier_to_uint160(address rd) internal _m_modifier_to_uint160(rd)  returns (uint256) { return _res_modifier_to_uint160; }
    function _w_ptr_to_uint160() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_address_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_address_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(address pv) internal view  returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    uint256 _res_modifier_to_address_payable;
    modifier _m_modifier_to_address_payable(address pv) { _res_modifier_to_address_payable = uint256(uint160(address(payable(address(pv))))); _; }
    function _e_modifier_to_address_payable(address rd) internal _m_modifier_to_address_payable(rd)  returns (uint256) { return _res_modifier_to_address_payable; }
    function _w_ptr_to_address_payable() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_to_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_to_payable() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_payable(address pv) internal view  returns (uint256) { return uint256(uint160(address(payable(pv)))); }
    uint256 _res_modifier_to_payable;
    modifier _m_modifier_to_payable(address pv) { _res_modifier_to_payable = uint256(uint160(address(payable(pv)))); _; }
    function _e_modifier_to_payable(address rd) internal _m_modifier_to_payable(rd)  returns (uint256) { return _res_modifier_to_payable; }
    function _w_ptr_to_payable() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_local_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_ret_to_uint160() external view  returns (uint256) { address rx = _s_ret_to_uint160(); return uint256(uint160(address(rx))); }
    function t_param_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_copy_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(ry))); }
    function t_ternary_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(ry))); }
    function t_ifjoin_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(ry))); }
    function t_loopjoin_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(ry))); }
    function t_tuple_to_uint160() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(ry))); }
    function t_modifier_to_uint160() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_uint160(rx); }
    function t_ptr_to_uint160() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint160; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(ry))); }
    function t_mem_to_uint160() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view  returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_ret_to_address_payable() external view  returns (uint256) { address rx = _s_ret_to_address_payable(); return uint256(uint160(address(payable(address(rx))))); }
    function t_param_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
    function t_copy_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(payable(address(ry))))); }
    function t_ternary_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(payable(address(ry))))); }
    function t_ifjoin_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(payable(address(ry))))); }
    function t_loopjoin_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(payable(address(ry))))); }
    function t_tuple_to_address_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(payable(address(ry))))); }
    function t_modifier_to_address_payable() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_address_payable(rx); }
    function t_ptr_to_address_payable() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_address_payable; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(payable(address(ry))))); }
    function t_mem_to_address_payable() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
    function t_local_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(rx)))); }
    function t_retdirect_to_payable() external view  returns (uint256) { return uint256(uint160(address(payable(_s_retdirect_to_payable())))); }
    function t_ret_to_payable() external view  returns (uint256) { address rx = _s_ret_to_payable(); return uint256(uint160(address(payable(rx)))); }
    function t_param_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_payable(rx); }
    function t_copy_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return uint256(uint160(address(payable(ry)))); }
    function t_ternary_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return uint256(uint160(address(payable(ry)))); }
    function t_ifjoin_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(address(payable(ry)))); }
    function t_loopjoin_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(address(payable(ry)))); }
    function t_tuple_to_payable() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(address(payable(ry)))); }
    function t_modifier_to_payable() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_to_payable(rx); }
    function t_ptr_to_payable() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_payable; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return uint256(uint160(address(payable(ry)))); }
    function t_mem_to_payable() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return uint256(uint160(address(payable(ry)))); }
}
contract Caddrp_0 {
    function _s_retdirect_to_uint160() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(address payable pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_address_payable() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(address payable pv) internal view  returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    function t_local_to_uint160() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_param_to_uint160() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_mem_to_uint160() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_address_payable() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view  returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_param_to_address_payable() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
    function t_mem_to_address_payable() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
}
contract Cctr_0 {
    function _s_retdirect_to_uint160() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_uint160(IC pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    function _s_retdirect_to_address_payable() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_address_payable(IC pv) internal view  returns (uint256) { return uint256(uint160(address(payable(address(pv))))); }
    function _s_retdirect_to_ic_addr() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_to_ic_addr(IC pv) internal view  returns (uint256) { return uint256(uint160(address(IC(address(pv))))); }
    function t_local_to_uint160() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(rx))); }
    function t_retdirect_to_uint160() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_uint160()))); }
    function t_param_to_uint160() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_uint160(rx); }
    function t_mem_to_uint160() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(ry))); }
    function t_local_to_address_payable() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(payable(address(rx))))); }
    function t_retdirect_to_address_payable() external view  returns (uint256) { return uint256(uint160(address(payable(address(_s_retdirect_to_address_payable()))))); }
    function t_param_to_address_payable() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_address_payable(rx); }
    function t_mem_to_address_payable() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(payable(address(ry))))); }
    function t_local_to_ic_addr() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return uint256(uint160(address(IC(address(rx))))); }
    function t_retdirect_to_ic_addr() external view  returns (uint256) { return uint256(uint160(address(IC(address(_s_retdirect_to_ic_addr()))))); }
    function t_param_to_ic_addr() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_to_ic_addr(rx); }
    function t_mem_to_ic_addr() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return uint256(uint160(address(IC(address(ry))))); }
}
contract Cb20_0 {
    function _s_retdirect_to_address() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_to_address(bytes20 pv) internal view  returns (uint256) { return uint256(uint160(address(pv))); }
    function t_local_to_address() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return uint256(uint160(address(rx))); }
    function t_retdirect_to_address() external view  returns (uint256) { return uint256(uint160(address(_s_retdirect_to_address()))); }
    function t_param_to_address() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_to_address(rx); }
    function t_mem_to_address() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; return uint256(uint160(address(ry))); }
}
