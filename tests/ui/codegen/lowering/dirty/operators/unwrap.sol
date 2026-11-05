//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Cen_0::t_local_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_to_uint => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_unwrap => 27
//@ run-call: Cud_0::t_retdirect_unwrap => 27
//@ run-call: Cud_0::t_param_unwrap => 27
//@ run-call: Cud_0::t_mem_unwrap => 27

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

contract Cen_0 {
    function _s_retdirect_to_uint() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_to_uint() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_to_uint(E pv) internal view  returns (uint256) { return uint256(uint8(pv)); }
    uint256 _res_modifier_to_uint;
    modifier _m_modifier_to_uint(E pv) { _res_modifier_to_uint = uint256(uint8(pv)); _; }
    function _e_modifier_to_uint(E rd) internal _m_modifier_to_uint(rd)  returns (uint256) { return _res_modifier_to_uint; }
    function _w_ptr_to_uint() internal pure returns (uint256) { return 0x101; }
    function t_local_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(uint8(rx)); }
    function t_retdirect_to_uint() external view  returns (uint256) { return uint256(uint8(_s_retdirect_to_uint())); }
    function t_ret_to_uint() external view  returns (uint256) { E rx = _s_ret_to_uint(); return uint256(uint8(rx)); }
    function t_param_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_to_uint(rx); }
    function t_copy_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(uint8(ry)); }
    function t_ternary_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(uint8(ry)); }
    function t_ifjoin_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint8(ry)); }
    function t_loopjoin_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint8(ry)); }
    function t_tuple_to_uint() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint8(ry)); }
    function t_modifier_to_uint() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_to_uint(rx); }
    function t_ptr_to_uint() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_to_uint; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(uint8(ry)); }
    function t_mem_to_uint() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(uint8(ry)); }
}
contract Cud_0 {
    function _s_retdirect_unwrap() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_unwrap(U pv) internal view  returns (uint256) { return uint256(U.unwrap(pv)); }
    function t_local_unwrap() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(U.unwrap(rx)); }
    function t_retdirect_unwrap() external view  returns (uint256) { return uint256(U.unwrap(_s_retdirect_unwrap())); }
    function t_param_unwrap() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_unwrap(rx); }
    function t_mem_unwrap() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(U.unwrap(ry)); }
}
