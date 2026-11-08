//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_addmod_a => 30
//@ run-call: Cu8_0::t_retdirect_addmod_a => 30
//@ run-call: Cu8_0::t_ret_addmod_a => 30
//@ run-call: Cu8_0::t_param_addmod_a => 30
//@ run-call: Cu8_0::t_copy_addmod_a => 30
//@ run-call: Cu8_0::t_ternary_addmod_a => 30
//@ run-call: Cu8_0::t_ifjoin_addmod_a => 30
//@ run-call: Cu8_0::t_loopjoin_addmod_a => 30
//@ run-call: Cu8_0::t_tuple_addmod_a => 30
//@ run-call: Cu8_0::t_modifier_addmod_a => 30
//@ run-call: Cu8_0::t_ptr_addmod_a => 30
//@ run-call: Cu8_0::t_mem_addmod_a => 30
//@ run-call: Cu8_0::t_local_addmod_k => 8
//@ run-call: Cu8_0::t_retdirect_addmod_k => 8
//@ run-call: Cu8_0::t_ret_addmod_k => 8
//@ run-call: Cu8_0::t_param_addmod_k => 8
//@ run-call: Cu8_0::t_copy_addmod_k => 8
//@ run-call: Cu8_0::t_ternary_addmod_k => 8
//@ run-call: Cu8_0::t_ifjoin_addmod_k => 8
//@ run-call: Cu8_0::t_loopjoin_addmod_k => 8
//@ run-call: Cu8_0::t_tuple_addmod_k => 8
//@ run-call: Cu8_0::t_modifier_addmod_k => 8
//@ run-call: Cu8_0::t_ptr_addmod_k => 8
//@ run-call: Cu8_0::t_mem_addmod_k => 8
//@ run-call: Cu8_0::t_local_mulmod_a => 81
//@ run-call: Cu8_0::t_retdirect_mulmod_a => 81
//@ run-call: Cu8_0::t_ret_mulmod_a => 81
//@ run-call: Cu8_0::t_param_mulmod_a => 81
//@ run-call: Cu8_0::t_copy_mulmod_a => 81
//@ run-call: Cu8_0::t_ternary_mulmod_a => 81
//@ run-call: Cu8_0::t_ifjoin_mulmod_a => 81
//@ run-call: Cu8_0::t_loopjoin_mulmod_a => 81
//@ run-call: Cu8_0::t_tuple_mulmod_a => 81
//@ run-call: Cu8_0::t_modifier_mulmod_a => 81
//@ run-call: Cu8_0::t_ptr_mulmod_a => 81
//@ run-call: Cu8_0::t_mem_mulmod_a => 81
//@ run-call: Cu8_0::t_local_mulmod_k => 15
//@ run-call: Cu8_0::t_retdirect_mulmod_k => 15
//@ run-call: Cu8_0::t_ret_mulmod_k => 15
//@ run-call: Cu8_0::t_param_mulmod_k => 15
//@ run-call: Cu8_1::t_copy_mulmod_k => 15
//@ run-call: Cu8_1::t_ternary_mulmod_k => 15
//@ run-call: Cu8_1::t_ifjoin_mulmod_k => 15
//@ run-call: Cu8_1::t_loopjoin_mulmod_k => 15
//@ run-call: Cu8_1::t_tuple_mulmod_k => 15
//@ run-call: Cu8_1::t_modifier_mulmod_k => 15
//@ run-call: Cu8_1::t_ptr_mulmod_k => 15
//@ run-call: Cu8_1::t_mem_mulmod_k => 15
//@ run-call: Cu8_1::t_local_addmod_b => 30
//@ run-call: Cu8_1::t_retdirect_addmod_b => 30
//@ run-call: Cu8_1::t_ret_addmod_b => 30
//@ run-call: Cu8_1::t_param_addmod_b => 30
//@ run-call: Cu8_1::t_copy_addmod_b => 30
//@ run-call: Cu8_1::t_ternary_addmod_b => 30
//@ run-call: Cu8_1::t_ifjoin_addmod_b => 30
//@ run-call: Cu8_1::t_loopjoin_addmod_b => 30
//@ run-call: Cu8_1::t_tuple_addmod_b => 30
//@ run-call: Cu8_1::t_modifier_addmod_b => 30
//@ run-call: Cu8_1::t_ptr_addmod_b => 30
//@ run-call: Cu8_1::t_mem_addmod_b => 30
//@ run-call: Cu8_1::t_local_mulmod_b => 81
//@ run-call: Cu8_1::t_retdirect_mulmod_b => 81
//@ run-call: Cu8_1::t_ret_mulmod_b => 81
//@ run-call: Cu8_1::t_param_mulmod_b => 81
//@ run-call: Cu8_1::t_copy_mulmod_b => 81
//@ run-call: Cu8_1::t_ternary_mulmod_b => 81
//@ run-call: Cu8_1::t_ifjoin_mulmod_b => 81
//@ run-call: Cu8_1::t_loopjoin_mulmod_b => 81
//@ run-call: Cu8_1::t_tuple_mulmod_b => 81
//@ run-call: Cu8_1::t_modifier_mulmod_b => 81
//@ run-call: Cu8_1::t_ptr_mulmod_b => 81
//@ run-call: Cu8_1::t_mem_mulmod_b => 81
//@ run-call: Cu64_0::t_local_addmod_a => 30
//@ run-call: Cu64_0::t_retdirect_addmod_a => 30
//@ run-call: Cu64_0::t_ret_addmod_a => 30
//@ run-call: Cu64_0::t_param_addmod_a => 30
//@ run-call: Cu64_0::t_copy_addmod_a => 30
//@ run-call: Cu64_0::t_ternary_addmod_a => 30
//@ run-call: Cu64_0::t_ifjoin_addmod_a => 30
//@ run-call: Cu64_0::t_loopjoin_addmod_a => 30
//@ run-call: Cu64_0::t_tuple_addmod_a => 30
//@ run-call: Cu64_0::t_modifier_addmod_a => 30
//@ run-call: Cu64_0::t_ptr_addmod_a => 30
//@ run-call: Cu64_0::t_mem_addmod_a => 30
//@ run-call: Cu64_0::t_local_addmod_k => 8
//@ run-call: Cu64_0::t_retdirect_addmod_k => 8
//@ run-call: Cu64_0::t_ret_addmod_k => 8
//@ run-call: Cu64_0::t_param_addmod_k => 8
//@ run-call: Cu64_0::t_copy_addmod_k => 8
//@ run-call: Cu64_0::t_ternary_addmod_k => 8
//@ run-call: Cu64_0::t_ifjoin_addmod_k => 8
//@ run-call: Cu64_0::t_loopjoin_addmod_k => 8
//@ run-call: Cu64_0::t_tuple_addmod_k => 8
//@ run-call: Cu64_0::t_modifier_addmod_k => 8
//@ run-call: Cu64_0::t_ptr_addmod_k => 8
//@ run-call: Cu64_0::t_mem_addmod_k => 8
//@ run-call: Cu64_0::t_local_mulmod_a => 81
//@ run-call: Cu64_0::t_retdirect_mulmod_a => 81
//@ run-call: Cu64_0::t_ret_mulmod_a => 81
//@ run-call: Cu64_0::t_param_mulmod_a => 81
//@ run-call: Cu64_0::t_copy_mulmod_a => 81
//@ run-call: Cu64_0::t_ternary_mulmod_a => 81
//@ run-call: Cu64_0::t_ifjoin_mulmod_a => 81
//@ run-call: Cu64_0::t_loopjoin_mulmod_a => 81
//@ run-call: Cu64_0::t_tuple_mulmod_a => 81
//@ run-call: Cu64_0::t_modifier_mulmod_a => 81
//@ run-call: Cu64_0::t_ptr_mulmod_a => 81
//@ run-call: Cu64_0::t_mem_mulmod_a => 81
//@ run-call: Cu64_0::t_local_mulmod_k => 15
//@ run-call: Cu64_0::t_retdirect_mulmod_k => 15
//@ run-call: Cu64_0::t_ret_mulmod_k => 15
//@ run-call: Cu64_0::t_param_mulmod_k => 15
//@ run-call: Cu64_1::t_copy_mulmod_k => 15
//@ run-call: Cu64_1::t_ternary_mulmod_k => 15
//@ run-call: Cu64_1::t_ifjoin_mulmod_k => 15
//@ run-call: Cu64_1::t_loopjoin_mulmod_k => 15
//@ run-call: Cu64_1::t_tuple_mulmod_k => 15
//@ run-call: Cu64_1::t_modifier_mulmod_k => 15
//@ run-call: Cu64_1::t_ptr_mulmod_k => 15
//@ run-call: Cu64_1::t_mem_mulmod_k => 15
//@ run-call: Cu64_1::t_local_addmod_b => 30
//@ run-call: Cu64_1::t_retdirect_addmod_b => 30
//@ run-call: Cu64_1::t_ret_addmod_b => 30
//@ run-call: Cu64_1::t_param_addmod_b => 30
//@ run-call: Cu64_1::t_copy_addmod_b => 30
//@ run-call: Cu64_1::t_ternary_addmod_b => 30
//@ run-call: Cu64_1::t_ifjoin_addmod_b => 30
//@ run-call: Cu64_1::t_loopjoin_addmod_b => 30
//@ run-call: Cu64_1::t_tuple_addmod_b => 30
//@ run-call: Cu64_1::t_modifier_addmod_b => 30
//@ run-call: Cu64_1::t_ptr_addmod_b => 30
//@ run-call: Cu64_1::t_mem_addmod_b => 30
//@ run-call: Cu64_1::t_local_mulmod_b => 81
//@ run-call: Cu64_1::t_retdirect_mulmod_b => 81
//@ run-call: Cu64_1::t_ret_mulmod_b => 81
//@ run-call: Cu64_1::t_param_mulmod_b => 81
//@ run-call: Cu64_1::t_copy_mulmod_b => 81
//@ run-call: Cu64_1::t_ternary_mulmod_b => 81
//@ run-call: Cu64_1::t_ifjoin_mulmod_b => 81
//@ run-call: Cu64_1::t_loopjoin_mulmod_b => 81
//@ run-call: Cu64_1::t_tuple_mulmod_b => 81
//@ run-call: Cu64_1::t_modifier_mulmod_b => 81
//@ run-call: Cu64_1::t_ptr_mulmod_b => 81
//@ run-call: Cu64_1::t_mem_mulmod_b => 81

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
    function _s_retdirect_addmod_a() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_addmod_a() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_addmod_a(uint8 pv) internal view  returns (uint256) { return addmod(pv, uint8(3), 100); }
    uint256 _res_modifier_addmod_a;
    modifier _m_modifier_addmod_a(uint8 pv) { _res_modifier_addmod_a = addmod(pv, uint8(3), 100); _; }
    function _e_modifier_addmod_a(uint8 rd) internal _m_modifier_addmod_a(rd)  returns (uint256) { return _res_modifier_addmod_a; }
    function _w_ptr_addmod_a() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_addmod_k() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_addmod_k() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_addmod_k(uint8 pv) internal view  returns (uint256) { return addmod(5, uint8(3), pv); }
    uint256 _res_modifier_addmod_k;
    modifier _m_modifier_addmod_k(uint8 pv) { _res_modifier_addmod_k = addmod(5, uint8(3), pv); _; }
    function _e_modifier_addmod_k(uint8 rd) internal _m_modifier_addmod_k(rd)  returns (uint256) { return _res_modifier_addmod_k; }
    function _w_ptr_addmod_k() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_mulmod_a() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_mulmod_a() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mulmod_a(uint8 pv) internal view  returns (uint256) { return mulmod(pv, uint8(3), 100); }
    uint256 _res_modifier_mulmod_a;
    modifier _m_modifier_mulmod_a(uint8 pv) { _res_modifier_mulmod_a = mulmod(pv, uint8(3), 100); _; }
    function _e_modifier_mulmod_a(uint8 rd) internal _m_modifier_mulmod_a(rd)  returns (uint256) { return _res_modifier_mulmod_a; }
    function _w_ptr_mulmod_a() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_mulmod_k() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_mulmod_k() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mulmod_k(uint8 pv) internal view  returns (uint256) { return mulmod(5, uint8(3), pv); }
    function t_local_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return addmod(rx, uint8(3), 100); }
    function t_retdirect_addmod_a() external view  returns (uint256) { return addmod(_s_retdirect_addmod_a(), uint8(3), 100); }
    function t_ret_addmod_a() external view  returns (uint256) { uint8 rx = _s_ret_addmod_a(); return addmod(rx, uint8(3), 100); }
    function t_param_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_addmod_a(rx); }
    function t_copy_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return addmod(ry, uint8(3), 100); }
    function t_ternary_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return addmod(ry, uint8(3), 100); }
    function t_ifjoin_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(ry, uint8(3), 100); }
    function t_loopjoin_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(ry, uint8(3), 100); }
    function t_tuple_addmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(ry, uint8(3), 100); }
    function t_modifier_addmod_a() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_addmod_a(rx); }
    function t_ptr_addmod_a() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_a; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return addmod(ry, uint8(3), 100); }
    function t_mem_addmod_a() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return addmod(ry, uint8(3), 100); }
    function t_local_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return addmod(5, uint8(3), rx); }
    function t_retdirect_addmod_k() external view  returns (uint256) { return addmod(5, uint8(3), _s_retdirect_addmod_k()); }
    function t_ret_addmod_k() external view  returns (uint256) { uint8 rx = _s_ret_addmod_k(); return addmod(5, uint8(3), rx); }
    function t_param_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_addmod_k(rx); }
    function t_copy_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return addmod(5, uint8(3), ry); }
    function t_ternary_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return addmod(5, uint8(3), ry); }
    function t_ifjoin_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(5, uint8(3), ry); }
    function t_loopjoin_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(5, uint8(3), ry); }
    function t_tuple_addmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(5, uint8(3), ry); }
    function t_modifier_addmod_k() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_addmod_k(rx); }
    function t_ptr_addmod_k() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_k; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return addmod(5, uint8(3), ry); }
    function t_mem_addmod_k() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return addmod(5, uint8(3), ry); }
    function t_local_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return mulmod(rx, uint8(3), 100); }
    function t_retdirect_mulmod_a() external view  returns (uint256) { return mulmod(_s_retdirect_mulmod_a(), uint8(3), 100); }
    function t_ret_mulmod_a() external view  returns (uint256) { uint8 rx = _s_ret_mulmod_a(); return mulmod(rx, uint8(3), 100); }
    function t_param_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mulmod_a(rx); }
    function t_copy_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return mulmod(ry, uint8(3), 100); }
    function t_ternary_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return mulmod(ry, uint8(3), 100); }
    function t_ifjoin_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(ry, uint8(3), 100); }
    function t_loopjoin_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(ry, uint8(3), 100); }
    function t_tuple_mulmod_a() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(ry, uint8(3), 100); }
    function t_modifier_mulmod_a() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_mulmod_a(rx); }
    function t_ptr_mulmod_a() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_a; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return mulmod(ry, uint8(3), 100); }
    function t_mem_mulmod_a() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return mulmod(ry, uint8(3), 100); }
    function t_local_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return mulmod(5, uint8(3), rx); }
    function t_retdirect_mulmod_k() external view  returns (uint256) { return mulmod(5, uint8(3), _s_retdirect_mulmod_k()); }
    function t_ret_mulmod_k() external view  returns (uint256) { uint8 rx = _s_ret_mulmod_k(); return mulmod(5, uint8(3), rx); }
    function t_param_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mulmod_k(rx); }
}
contract Cu8_1 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    uint256 _res_modifier_mulmod_k;
    modifier _m_modifier_mulmod_k(uint8 pv) { _res_modifier_mulmod_k = mulmod(5, uint8(3), pv); _; }
    function _e_modifier_mulmod_k(uint8 rd) internal _m_modifier_mulmod_k(rd)  returns (uint256) { return _res_modifier_mulmod_k; }
    function _w_ptr_mulmod_k() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_addmod_b() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_addmod_b() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_addmod_b(uint8 pv) internal view  returns (uint256) { return addmod(uint8(3), pv, 100); }
    uint256 _res_modifier_addmod_b;
    modifier _m_modifier_addmod_b(uint8 pv) { _res_modifier_addmod_b = addmod(uint8(3), pv, 100); _; }
    function _e_modifier_addmod_b(uint8 rd) internal _m_modifier_addmod_b(rd)  returns (uint256) { return _res_modifier_addmod_b; }
    function _w_ptr_addmod_b() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_mulmod_b() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_mulmod_b() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mulmod_b(uint8 pv) internal view  returns (uint256) { return mulmod(uint8(3), pv, 100); }
    uint256 _res_modifier_mulmod_b;
    modifier _m_modifier_mulmod_b(uint8 pv) { _res_modifier_mulmod_b = mulmod(uint8(3), pv, 100); _; }
    function _e_modifier_mulmod_b(uint8 rd) internal _m_modifier_mulmod_b(rd)  returns (uint256) { return _res_modifier_mulmod_b; }
    function _w_ptr_mulmod_b() internal pure returns (uint256) { return 0xabcd1b; }
    function t_copy_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return mulmod(5, uint8(3), ry); }
    function t_ternary_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return mulmod(5, uint8(3), ry); }
    function t_ifjoin_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(5, uint8(3), ry); }
    function t_loopjoin_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(5, uint8(3), ry); }
    function t_tuple_mulmod_k() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(5, uint8(3), ry); }
    function t_modifier_mulmod_k() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_mulmod_k(rx); }
    function t_ptr_mulmod_k() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_k; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return mulmod(5, uint8(3), ry); }
    function t_mem_mulmod_k() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return mulmod(5, uint8(3), ry); }
    function t_local_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return addmod(uint8(3), rx, 100); }
    function t_retdirect_addmod_b() external view  returns (uint256) { return addmod(uint8(3), _s_retdirect_addmod_b(), 100); }
    function t_ret_addmod_b() external view  returns (uint256) { uint8 rx = _s_ret_addmod_b(); return addmod(uint8(3), rx, 100); }
    function t_param_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_addmod_b(rx); }
    function t_copy_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return addmod(uint8(3), ry, 100); }
    function t_ternary_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return addmod(uint8(3), ry, 100); }
    function t_ifjoin_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(uint8(3), ry, 100); }
    function t_loopjoin_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(uint8(3), ry, 100); }
    function t_tuple_addmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(uint8(3), ry, 100); }
    function t_modifier_addmod_b() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_addmod_b(rx); }
    function t_ptr_addmod_b() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_b; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return addmod(uint8(3), ry, 100); }
    function t_mem_addmod_b() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return addmod(uint8(3), ry, 100); }
    function t_local_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return mulmod(uint8(3), rx, 100); }
    function t_retdirect_mulmod_b() external view  returns (uint256) { return mulmod(uint8(3), _s_retdirect_mulmod_b(), 100); }
    function t_ret_mulmod_b() external view  returns (uint256) { uint8 rx = _s_ret_mulmod_b(); return mulmod(uint8(3), rx, 100); }
    function t_param_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mulmod_b(rx); }
    function t_copy_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return mulmod(uint8(3), ry, 100); }
    function t_ternary_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return mulmod(uint8(3), ry, 100); }
    function t_ifjoin_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(uint8(3), ry, 100); }
    function t_loopjoin_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(uint8(3), ry, 100); }
    function t_tuple_mulmod_b() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(uint8(3), ry, 100); }
    function t_modifier_mulmod_b() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_mulmod_b(rx); }
    function t_ptr_mulmod_b() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_b; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return mulmod(uint8(3), ry, 100); }
    function t_mem_mulmod_b() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return mulmod(uint8(3), ry, 100); }
}
contract Cu64_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_addmod_a() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_addmod_a() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_addmod_a(uint64 pv) internal view  returns (uint256) { return addmod(pv, uint64(3), 100); }
    uint256 _res_modifier_addmod_a;
    modifier _m_modifier_addmod_a(uint64 pv) { _res_modifier_addmod_a = addmod(pv, uint64(3), 100); _; }
    function _e_modifier_addmod_a(uint64 rd) internal _m_modifier_addmod_a(rd)  returns (uint256) { return _res_modifier_addmod_a; }
    function _w_ptr_addmod_a() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_addmod_k() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_addmod_k() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_addmod_k(uint64 pv) internal view  returns (uint256) { return addmod(5, uint64(3), pv); }
    uint256 _res_modifier_addmod_k;
    modifier _m_modifier_addmod_k(uint64 pv) { _res_modifier_addmod_k = addmod(5, uint64(3), pv); _; }
    function _e_modifier_addmod_k(uint64 rd) internal _m_modifier_addmod_k(rd)  returns (uint256) { return _res_modifier_addmod_k; }
    function _w_ptr_addmod_k() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_mulmod_a() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_mulmod_a() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mulmod_a(uint64 pv) internal view  returns (uint256) { return mulmod(pv, uint64(3), 100); }
    uint256 _res_modifier_mulmod_a;
    modifier _m_modifier_mulmod_a(uint64 pv) { _res_modifier_mulmod_a = mulmod(pv, uint64(3), 100); _; }
    function _e_modifier_mulmod_a(uint64 rd) internal _m_modifier_mulmod_a(rd)  returns (uint256) { return _res_modifier_mulmod_a; }
    function _w_ptr_mulmod_a() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_mulmod_k() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_mulmod_k() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mulmod_k(uint64 pv) internal view  returns (uint256) { return mulmod(5, uint64(3), pv); }
    function t_local_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return addmod(rx, uint64(3), 100); }
    function t_retdirect_addmod_a() external view  returns (uint256) { return addmod(_s_retdirect_addmod_a(), uint64(3), 100); }
    function t_ret_addmod_a() external view  returns (uint256) { uint64 rx = _s_ret_addmod_a(); return addmod(rx, uint64(3), 100); }
    function t_param_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_addmod_a(rx); }
    function t_copy_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return addmod(ry, uint64(3), 100); }
    function t_ternary_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return addmod(ry, uint64(3), 100); }
    function t_ifjoin_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(ry, uint64(3), 100); }
    function t_loopjoin_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(ry, uint64(3), 100); }
    function t_tuple_addmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(ry, uint64(3), 100); }
    function t_modifier_addmod_a() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_addmod_a(rx); }
    function t_ptr_addmod_a() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_a; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return addmod(ry, uint64(3), 100); }
    function t_mem_addmod_a() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return addmod(ry, uint64(3), 100); }
    function t_local_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return addmod(5, uint64(3), rx); }
    function t_retdirect_addmod_k() external view  returns (uint256) { return addmod(5, uint64(3), _s_retdirect_addmod_k()); }
    function t_ret_addmod_k() external view  returns (uint256) { uint64 rx = _s_ret_addmod_k(); return addmod(5, uint64(3), rx); }
    function t_param_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_addmod_k(rx); }
    function t_copy_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return addmod(5, uint64(3), ry); }
    function t_ternary_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return addmod(5, uint64(3), ry); }
    function t_ifjoin_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(5, uint64(3), ry); }
    function t_loopjoin_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(5, uint64(3), ry); }
    function t_tuple_addmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(5, uint64(3), ry); }
    function t_modifier_addmod_k() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_addmod_k(rx); }
    function t_ptr_addmod_k() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_k; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return addmod(5, uint64(3), ry); }
    function t_mem_addmod_k() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return addmod(5, uint64(3), ry); }
    function t_local_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return mulmod(rx, uint64(3), 100); }
    function t_retdirect_mulmod_a() external view  returns (uint256) { return mulmod(_s_retdirect_mulmod_a(), uint64(3), 100); }
    function t_ret_mulmod_a() external view  returns (uint256) { uint64 rx = _s_ret_mulmod_a(); return mulmod(rx, uint64(3), 100); }
    function t_param_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mulmod_a(rx); }
    function t_copy_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return mulmod(ry, uint64(3), 100); }
    function t_ternary_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return mulmod(ry, uint64(3), 100); }
    function t_ifjoin_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(ry, uint64(3), 100); }
    function t_loopjoin_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(ry, uint64(3), 100); }
    function t_tuple_mulmod_a() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(ry, uint64(3), 100); }
    function t_modifier_mulmod_a() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_mulmod_a(rx); }
    function t_ptr_mulmod_a() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_a; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return mulmod(ry, uint64(3), 100); }
    function t_mem_mulmod_a() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return mulmod(ry, uint64(3), 100); }
    function t_local_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return mulmod(5, uint64(3), rx); }
    function t_retdirect_mulmod_k() external view  returns (uint256) { return mulmod(5, uint64(3), _s_retdirect_mulmod_k()); }
    function t_ret_mulmod_k() external view  returns (uint256) { uint64 rx = _s_ret_mulmod_k(); return mulmod(5, uint64(3), rx); }
    function t_param_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mulmod_k(rx); }
}
contract Cu64_1 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    uint256 _res_modifier_mulmod_k;
    modifier _m_modifier_mulmod_k(uint64 pv) { _res_modifier_mulmod_k = mulmod(5, uint64(3), pv); _; }
    function _e_modifier_mulmod_k(uint64 rd) internal _m_modifier_mulmod_k(rd)  returns (uint256) { return _res_modifier_mulmod_k; }
    function _w_ptr_mulmod_k() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_addmod_b() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_addmod_b() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_addmod_b(uint64 pv) internal view  returns (uint256) { return addmod(uint64(3), pv, 100); }
    uint256 _res_modifier_addmod_b;
    modifier _m_modifier_addmod_b(uint64 pv) { _res_modifier_addmod_b = addmod(uint64(3), pv, 100); _; }
    function _e_modifier_addmod_b(uint64 rd) internal _m_modifier_addmod_b(rd)  returns (uint256) { return _res_modifier_addmod_b; }
    function _w_ptr_addmod_b() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function _s_retdirect_mulmod_b() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_mulmod_b() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mulmod_b(uint64 pv) internal view  returns (uint256) { return mulmod(uint64(3), pv, 100); }
    uint256 _res_modifier_mulmod_b;
    modifier _m_modifier_mulmod_b(uint64 pv) { _res_modifier_mulmod_b = mulmod(uint64(3), pv, 100); _; }
    function _e_modifier_mulmod_b(uint64 rd) internal _m_modifier_mulmod_b(rd)  returns (uint256) { return _res_modifier_mulmod_b; }
    function _w_ptr_mulmod_b() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function t_copy_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return mulmod(5, uint64(3), ry); }
    function t_ternary_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return mulmod(5, uint64(3), ry); }
    function t_ifjoin_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(5, uint64(3), ry); }
    function t_loopjoin_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(5, uint64(3), ry); }
    function t_tuple_mulmod_k() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(5, uint64(3), ry); }
    function t_modifier_mulmod_k() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_mulmod_k(rx); }
    function t_ptr_mulmod_k() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_k; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return mulmod(5, uint64(3), ry); }
    function t_mem_mulmod_k() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return mulmod(5, uint64(3), ry); }
    function t_local_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return addmod(uint64(3), rx, 100); }
    function t_retdirect_addmod_b() external view  returns (uint256) { return addmod(uint64(3), _s_retdirect_addmod_b(), 100); }
    function t_ret_addmod_b() external view  returns (uint256) { uint64 rx = _s_ret_addmod_b(); return addmod(uint64(3), rx, 100); }
    function t_param_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_addmod_b(rx); }
    function t_copy_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return addmod(uint64(3), ry, 100); }
    function t_ternary_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return addmod(uint64(3), ry, 100); }
    function t_ifjoin_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return addmod(uint64(3), ry, 100); }
    function t_loopjoin_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return addmod(uint64(3), ry, 100); }
    function t_tuple_addmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return addmod(uint64(3), ry, 100); }
    function t_modifier_addmod_b() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_addmod_b(rx); }
    function t_ptr_addmod_b() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_addmod_b; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return addmod(uint64(3), ry, 100); }
    function t_mem_addmod_b() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return addmod(uint64(3), ry, 100); }
    function t_local_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return mulmod(uint64(3), rx, 100); }
    function t_retdirect_mulmod_b() external view  returns (uint256) { return mulmod(uint64(3), _s_retdirect_mulmod_b(), 100); }
    function t_ret_mulmod_b() external view  returns (uint256) { uint64 rx = _s_ret_mulmod_b(); return mulmod(uint64(3), rx, 100); }
    function t_param_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mulmod_b(rx); }
    function t_copy_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return mulmod(uint64(3), ry, 100); }
    function t_ternary_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return mulmod(uint64(3), ry, 100); }
    function t_ifjoin_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return mulmod(uint64(3), ry, 100); }
    function t_loopjoin_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return mulmod(uint64(3), ry, 100); }
    function t_tuple_mulmod_b() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return mulmod(uint64(3), ry, 100); }
    function t_modifier_mulmod_b() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_mulmod_b(rx); }
    function t_ptr_mulmod_b() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_mulmod_b; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return mulmod(uint64(3), ry, 100); }
    function t_mem_mulmod_b() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return mulmod(uint64(3), ry, 100); }
}
