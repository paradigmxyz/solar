//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_cmp_eq => 0
//@ run-call: Caddr_0::t_retdirect_cmp_eq => 0
//@ run-call: Caddr_0::t_ret_cmp_eq => 0
//@ run-call: Caddr_0::t_param_cmp_eq => 0
//@ run-call: Caddr_0::t_copy_cmp_eq => 0
//@ run-call: Caddr_0::t_ternary_cmp_eq => 0
//@ run-call: Caddr_0::t_ifjoin_cmp_eq => 0
//@ run-call: Caddr_0::t_loopjoin_cmp_eq => 0
//@ run-call: Caddr_0::t_tuple_cmp_eq => 0
//@ run-call: Caddr_0::t_modifier_cmp_eq => 0
//@ run-call: Caddr_0::t_ptr_cmp_eq => 0
//@ run-call: Caddr_0::t_mem_cmp_eq => 0
//@ run-call: Caddr_0::t_local_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_retdirect_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_ret_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_param_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_copy_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_ternary_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_ifjoin_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_loopjoin_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_tuple_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_modifier_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_ptr_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_mem_cmp_eq_valid => 1
//@ run-call: Caddr_0::t_local_cmp_ne => 1
//@ run-call: Caddr_0::t_retdirect_cmp_ne => 1
//@ run-call: Caddr_0::t_ret_cmp_ne => 1
//@ run-call: Caddr_0::t_param_cmp_ne => 1
//@ run-call: Caddr_0::t_copy_cmp_ne => 1
//@ run-call: Caddr_0::t_ternary_cmp_ne => 1
//@ run-call: Caddr_0::t_ifjoin_cmp_ne => 1
//@ run-call: Caddr_0::t_loopjoin_cmp_ne => 1
//@ run-call: Caddr_0::t_tuple_cmp_ne => 1
//@ run-call: Caddr_0::t_modifier_cmp_ne => 1
//@ run-call: Caddr_0::t_ptr_cmp_ne => 1
//@ run-call: Caddr_0::t_mem_cmp_ne => 1
//@ run-call: Caddr_0::t_local_cmp_ne_valid => 0
//@ run-call: Caddr_0::t_retdirect_cmp_ne_valid => 0
//@ run-call: Caddr_0::t_ret_cmp_ne_valid => 0
//@ run-call: Caddr_0::t_param_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_copy_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_ternary_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_ifjoin_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_loopjoin_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_tuple_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_modifier_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_ptr_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_mem_cmp_ne_valid => 0
//@ run-call: Caddr_1::t_local_cmp_typed => 0
//@ run-call: Caddr_1::t_retdirect_cmp_typed => 0
//@ run-call: Caddr_1::t_ret_cmp_typed => 0
//@ run-call: Caddr_1::t_param_cmp_typed => 0
//@ run-call: Caddr_1::t_copy_cmp_typed => 0
//@ run-call: Caddr_1::t_ternary_cmp_typed => 0
//@ run-call: Caddr_1::t_ifjoin_cmp_typed => 0
//@ run-call: Caddr_1::t_loopjoin_cmp_typed => 0
//@ run-call: Caddr_1::t_tuple_cmp_typed => 0
//@ run-call: Caddr_1::t_modifier_cmp_typed => 0
//@ run-call: Caddr_1::t_ptr_cmp_typed => 0
//@ run-call: Caddr_1::t_mem_cmp_typed => 0
//@ run-call: Caddr_1::t_local_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_retdirect_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_ret_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_param_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_copy_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_ternary_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_ifjoin_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_loopjoin_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_tuple_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_modifier_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_ptr_cmp_typed_valid => 1
//@ run-call: Caddr_1::t_mem_cmp_typed_valid => 1
//@ run-call: Caddrp_0::t_local_cmp_eq => 0
//@ run-call: Caddrp_0::t_retdirect_cmp_eq => 0
//@ run-call: Caddrp_0::t_param_cmp_eq => 0
//@ run-call: Caddrp_0::t_mem_cmp_eq => 0
//@ run-call: Caddrp_0::t_local_cmp_eq_valid => 1
//@ run-call: Caddrp_0::t_retdirect_cmp_eq_valid => 1
//@ run-call: Caddrp_0::t_param_cmp_eq_valid => 1
//@ run-call: Caddrp_0::t_mem_cmp_eq_valid => 1
//@ run-call: Caddrp_0::t_local_cmp_ne => 1
//@ run-call: Caddrp_0::t_retdirect_cmp_ne => 1
//@ run-call: Caddrp_0::t_param_cmp_ne => 1
//@ run-call: Caddrp_0::t_mem_cmp_ne => 1
//@ run-call: Caddrp_0::t_local_cmp_ne_valid => 0
//@ run-call: Caddrp_0::t_retdirect_cmp_ne_valid => 0
//@ run-call: Caddrp_0::t_param_cmp_ne_valid => 0
//@ run-call: Caddrp_0::t_mem_cmp_ne_valid => 0
//@ run-call: Caddrp_0::t_local_cmp_typed => 0
//@ run-call: Caddrp_0::t_retdirect_cmp_typed => 0
//@ run-call: Caddrp_0::t_param_cmp_typed => 0
//@ run-call: Caddrp_0::t_mem_cmp_typed => 0
//@ run-call: Caddrp_0::t_local_cmp_typed_valid => 1
//@ run-call: Caddrp_0::t_retdirect_cmp_typed_valid => 1
//@ run-call: Caddrp_0::t_param_cmp_typed_valid => 1
//@ run-call: Caddrp_0::t_mem_cmp_typed_valid => 1
//@ run-call: Cctr_0::t_local_cmp_eq => 0
//@ run-call: Cctr_0::t_retdirect_cmp_eq => 0
//@ run-call: Cctr_0::t_param_cmp_eq => 0
//@ run-call: Cctr_0::t_mem_cmp_eq => 0
//@ run-call: Cctr_0::t_local_cmp_eq_valid => 1
//@ run-call: Cctr_0::t_retdirect_cmp_eq_valid => 1
//@ run-call: Cctr_0::t_param_cmp_eq_valid => 1
//@ run-call: Cctr_0::t_mem_cmp_eq_valid => 1
//@ run-call: Cctr_0::t_local_cmp_ne => 1
//@ run-call: Cctr_0::t_retdirect_cmp_ne => 1
//@ run-call: Cctr_0::t_param_cmp_ne => 1
//@ run-call: Cctr_0::t_mem_cmp_ne => 1
//@ run-call: Cctr_0::t_local_cmp_ne_valid => 0
//@ run-call: Cctr_0::t_retdirect_cmp_ne_valid => 0
//@ run-call: Cctr_0::t_param_cmp_ne_valid => 0
//@ run-call: Cctr_0::t_mem_cmp_ne_valid => 0
//@ run-call: Cctr_0::t_local_cmp_typed => 0
//@ run-call: Cctr_0::t_retdirect_cmp_typed => 0
//@ run-call: Cctr_0::t_param_cmp_typed => 0
//@ run-call: Cctr_0::t_mem_cmp_typed => 0
//@ run-call: Cctr_0::t_local_cmp_typed_valid => 1
//@ run-call: Cctr_0::t_retdirect_cmp_typed_valid => 1
//@ run-call: Cctr_0::t_param_cmp_typed_valid => 1
//@ run-call: Cctr_0::t_mem_cmp_typed_valid => 1

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

contract Caddr_0 {
    function _s_retdirect_cmp_eq() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_eq() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq(address pv) internal view  returns (uint256) { return (address(pv) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    uint256 _res_modifier_cmp_eq;
    modifier _m_modifier_cmp_eq(address pv) { _res_modifier_cmp_eq = (address(pv) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; _; }
    function _e_modifier_cmp_eq(address rd) internal _m_modifier_cmp_eq(rd)  returns (uint256) { return _res_modifier_cmp_eq; }
    function _w_ptr_cmp_eq() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_cmp_eq_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_eq_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq_valid(address pv) internal view  returns (uint256) { return (address(pv) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    uint256 _res_modifier_cmp_eq_valid;
    modifier _m_modifier_cmp_eq_valid(address pv) { _res_modifier_cmp_eq_valid = (address(pv) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; _; }
    function _e_modifier_cmp_eq_valid(address rd) internal _m_modifier_cmp_eq_valid(rd)  returns (uint256) { return _res_modifier_cmp_eq_valid; }
    function _w_ptr_cmp_eq_valid() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_cmp_ne() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_ne() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne(address pv) internal view  returns (uint256) { return (address(pv) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    uint256 _res_modifier_cmp_ne;
    modifier _m_modifier_cmp_ne(address pv) { _res_modifier_cmp_ne = (address(pv) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; _; }
    function _e_modifier_cmp_ne(address rd) internal _m_modifier_cmp_ne(rd)  returns (uint256) { return _res_modifier_cmp_ne; }
    function _w_ptr_cmp_ne() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_cmp_ne_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_ne_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne_valid(address pv) internal view  returns (uint256) { return (address(pv) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq()) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ret_cmp_eq() external view  returns (uint256) { address rx = _s_ret_cmp_eq(); return (address(rx) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq(rx); }
    function t_copy_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ternary_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ifjoin_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_loopjoin_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_tuple_cmp_eq() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_modifier_cmp_eq() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_eq(rx); }
    function t_ptr_cmp_eq() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_eq; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_mem_cmp_eq() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (address(ry) == address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_local_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_retdirect_cmp_eq_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq_valid()) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ret_cmp_eq_valid() external view  returns (uint256) { address rx = _s_ret_cmp_eq_valid(); return (address(rx) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_param_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq_valid(rx); }
    function t_copy_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ternary_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ifjoin_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_loopjoin_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_tuple_cmp_eq_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_modifier_cmp_eq_valid() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_eq_valid(rx); }
    function t_ptr_cmp_eq_valid() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_eq_valid; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_mem_cmp_eq_valid() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (address(ry) == address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne()) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ret_cmp_ne() external view  returns (uint256) { address rx = _s_ret_cmp_ne(); return (address(rx) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne(rx); }
    function t_copy_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ternary_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_ifjoin_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_loopjoin_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_tuple_cmp_ne() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_modifier_cmp_ne() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_ne(rx); }
    function t_ptr_cmp_ne() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_ne; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_mem_cmp_ne() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (address(ry) != address(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_local_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_retdirect_cmp_ne_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne_valid()) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ret_cmp_ne_valid() external view  returns (uint256) { address rx = _s_ret_cmp_ne_valid(); return (address(rx) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_param_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne_valid(rx); }
}
contract Caddr_1 {
    uint256 _res_modifier_cmp_ne_valid;
    modifier _m_modifier_cmp_ne_valid(address pv) { _res_modifier_cmp_ne_valid = (address(pv) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; _; }
    function _e_modifier_cmp_ne_valid(address rd) internal _m_modifier_cmp_ne_valid(rd)  returns (uint256) { return _res_modifier_cmp_ne_valid; }
    function _w_ptr_cmp_ne_valid() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_cmp_typed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_typed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed(address pv) internal view  returns (uint256) { return (pv == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    uint256 _res_modifier_cmp_typed;
    modifier _m_modifier_cmp_typed(address pv) { _res_modifier_cmp_typed = (pv == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; _; }
    function _e_modifier_cmp_typed(address rd) internal _m_modifier_cmp_typed(rd)  returns (uint256) { return _res_modifier_cmp_typed; }
    function _w_ptr_cmp_typed() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function _s_retdirect_cmp_typed_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_cmp_typed_valid() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed_valid(address pv) internal view  returns (uint256) { return (pv == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    uint256 _res_modifier_cmp_typed_valid;
    modifier _m_modifier_cmp_typed_valid(address pv) { _res_modifier_cmp_typed_valid = (pv == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; _; }
    function _e_modifier_cmp_typed_valid(address rd) internal _m_modifier_cmp_typed_valid(rd)  returns (uint256) { return _res_modifier_cmp_typed_valid; }
    function _w_ptr_cmp_typed_valid() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_copy_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ternary_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_ifjoin_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_loopjoin_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_tuple_cmp_ne_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_modifier_cmp_ne_valid() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_ne_valid(rx); }
    function t_ptr_cmp_ne_valid() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_ne_valid; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_mem_cmp_ne_valid() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (address(ry) != address(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_local_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_retdirect_cmp_typed() external view  returns (uint256) { return (_s_retdirect_cmp_typed() == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_ret_cmp_typed() external view  returns (uint256) { address rx = _s_ret_cmp_typed(); return (rx == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_param_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed(rx); }
    function t_copy_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_ternary_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_ifjoin_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_loopjoin_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_tuple_cmp_typed() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_modifier_cmp_typed() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_typed(rx); }
    function t_ptr_cmp_typed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_typed; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_mem_cmp_typed() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (ry == address(0x1111111111111111111111111111111111111111)) ? 1 : 0; }
    function t_local_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_retdirect_cmp_typed_valid() external view  returns (uint256) { return (_s_retdirect_cmp_typed_valid() == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_ret_cmp_typed_valid() external view  returns (uint256) { address rx = _s_ret_cmp_typed_valid(); return (rx == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_param_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed_valid(rx); }
    function t_copy_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_ternary_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_ifjoin_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_loopjoin_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_tuple_cmp_typed_valid() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_modifier_cmp_typed_valid() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_cmp_typed_valid(rx); }
    function t_ptr_cmp_typed_valid() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_cmp_typed_valid; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
    function t_mem_cmp_typed_valid() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; return (ry == address(0x1234567890123456789012345678901234567890)) ? 1 : 0; }
}
contract Caddrp_0 {
    function _s_retdirect_cmp_eq() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq(address payable pv) internal view  returns (uint256) { return (address(pv) == address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function _s_retdirect_cmp_eq_valid() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq_valid(address payable pv) internal view  returns (uint256) { return (address(pv) == address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function _s_retdirect_cmp_ne() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne(address payable pv) internal view  returns (uint256) { return (address(pv) != address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function _s_retdirect_cmp_ne_valid() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne_valid(address payable pv) internal view  returns (uint256) { return (address(pv) != address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function _s_retdirect_cmp_typed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed(address payable pv) internal view  returns (uint256) { return (pv == payable(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function _s_retdirect_cmp_typed_valid() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed_valid(address payable pv) internal view  returns (uint256) { return (pv == payable(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq()) == address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (address(ry) == address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_local_cmp_eq_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_retdirect_cmp_eq_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq_valid()) == address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_param_cmp_eq_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq_valid(rx); }
    function t_mem_cmp_eq_valid() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (address(ry) == address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne()) != address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne(rx); }
    function t_mem_cmp_ne() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (address(ry) != address(payable(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_local_cmp_ne_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_retdirect_cmp_ne_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne_valid()) != address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_param_cmp_ne_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne_valid(rx); }
    function t_mem_cmp_ne_valid() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (address(ry) != address(payable(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_local_cmp_typed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == payable(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_retdirect_cmp_typed() external view  returns (uint256) { return (_s_retdirect_cmp_typed() == payable(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_param_cmp_typed() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed(rx); }
    function t_mem_cmp_typed() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (ry == payable(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_local_cmp_typed_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == payable(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_retdirect_cmp_typed_valid() external view  returns (uint256) { return (_s_retdirect_cmp_typed_valid() == payable(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_param_cmp_typed_valid() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed_valid(rx); }
    function t_mem_cmp_typed_valid() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; return (ry == payable(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
}
contract Cctr_0 {
    function _s_retdirect_cmp_eq() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq(IC pv) internal view  returns (uint256) { return (address(pv) == address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function _s_retdirect_cmp_eq_valid() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_eq_valid(IC pv) internal view  returns (uint256) { return (address(pv) == address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function _s_retdirect_cmp_ne() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne(IC pv) internal view  returns (uint256) { return (address(pv) != address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function _s_retdirect_cmp_ne_valid() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_ne_valid(IC pv) internal view  returns (uint256) { return (address(pv) != address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function _s_retdirect_cmp_typed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed(IC pv) internal view  returns (uint256) { return (pv == IC(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function _s_retdirect_cmp_typed_valid() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_cmp_typed_valid(IC pv) internal view  returns (uint256) { return (pv == IC(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_local_cmp_eq() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_retdirect_cmp_eq() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq()) == address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_param_cmp_eq() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq(rx); }
    function t_mem_cmp_eq() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (address(ry) == address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_local_cmp_eq_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) == address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_retdirect_cmp_eq_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_eq_valid()) == address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_param_cmp_eq_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_eq_valid(rx); }
    function t_mem_cmp_eq_valid() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (address(ry) == address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_local_cmp_ne() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_retdirect_cmp_ne() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne()) != address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_param_cmp_ne() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne(rx); }
    function t_mem_cmp_ne() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (address(ry) != address(IC(address(0x1111111111111111111111111111111111111111)))) ? 1 : 0; }
    function t_local_cmp_ne_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (address(rx) != address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_retdirect_cmp_ne_valid() external view  returns (uint256) { return (address(_s_retdirect_cmp_ne_valid()) != address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_param_cmp_ne_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_ne_valid(rx); }
    function t_mem_cmp_ne_valid() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (address(ry) != address(IC(address(0x1234567890123456789012345678901234567890)))) ? 1 : 0; }
    function t_local_cmp_typed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == IC(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_retdirect_cmp_typed() external view  returns (uint256) { return (_s_retdirect_cmp_typed() == IC(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_param_cmp_typed() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed(rx); }
    function t_mem_cmp_typed() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (ry == IC(address(0x1111111111111111111111111111111111111111))) ? 1 : 0; }
    function t_local_cmp_typed_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return (rx == IC(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_retdirect_cmp_typed_valid() external view  returns (uint256) { return (_s_retdirect_cmp_typed_valid() == IC(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
    function t_param_cmp_typed_valid() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_cmp_typed_valid(rx); }
    function t_mem_cmp_typed_valid() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; return (ry == IC(address(0x1234567890123456789012345678901234567890))) ? 1 : 0; }
}
