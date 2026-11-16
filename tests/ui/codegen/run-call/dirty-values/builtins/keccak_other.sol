//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_retdirect_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_ret_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_param_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_copy_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_ternary_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_ifjoin_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_loopjoin_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_tuple_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_modifier_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_ptr_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_mem_keccak_packed => 43379584054787486383572605962602545002668015983485933488536749112829893476306
//@ run-call: Cbool_0::t_local_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_retdirect_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_ret_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_param_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_copy_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_ternary_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_ifjoin_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_loopjoin_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_tuple_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_modifier_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_ptr_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call: Cbool_0::t_mem_keccak_packed_two => 33386743121253133998044262846288522400453930703562266361888433873909073660373
//@ run-call-fail: Cen_0::t_local_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_keccak_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ternary_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_keccak_packed_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_retdirect_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_ret_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_param_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_copy_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_ternary_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_ifjoin_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_loopjoin_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_tuple_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_modifier_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_ptr_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_mem_keccak_packed => 16662853100387288873482409453919396373682617046379706899923604012247482681798
//@ run-call: Cud_0::t_local_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_retdirect_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_ret_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_param_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_copy_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_ternary_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_ifjoin_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_loopjoin_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_tuple_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_modifier_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_ptr_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207
//@ run-call: Cud_0::t_mem_keccak_packed_two => 111561061836250076686758725937105633709270419367507212174788428598309384727207

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

contract Cbool_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _s_ret_keccak_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_keccak_packed(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(bool pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(bool rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0x100; }
    function _s_retdirect_keccak_packed_two() internal view returns (bool r) { assembly { r := 0x100 } }
    function _s_ret_keccak_packed_two() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_keccak_packed_two(bool pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(false, pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(bool pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(false, pv))); _; }
    function _e_modifier_keccak_packed_two(bool rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0x100; }
    function t_local_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { bool rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = false; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } (bool ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (bool) rq; assembly { rq := rp } bool ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return uint256(keccak256(abi.encodePacked(false, rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(false, _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { bool rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(false, rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = rx; return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool rf = gasleft() > 0; bool ro = false; bool ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool ry = false; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } (bool ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { bool rx; assembly { rx := 0x100 } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (bool) rq; assembly { rq := rp } bool ry = rq(); return uint256(keccak256(abi.encodePacked(false, ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; return uint256(keccak256(abi.encodePacked(false, ry))); }
}
contract Cen_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_keccak_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_keccak_packed(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(E pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(E rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0x101; }
    function _s_retdirect_keccak_packed_two() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_keccak_packed_two() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_keccak_packed_two(E pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(E.A, pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(E pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(E.A, pv))); _; }
    function _e_modifier_keccak_packed_two(E rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0x101; }
    function t_local_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { E rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return uint256(keccak256(abi.encodePacked(E.A, rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(E.A, _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { E rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(E.A, rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); return uint256(keccak256(abi.encodePacked(E.A, ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; return uint256(keccak256(abi.encodePacked(E.A, ry))); }
}
contract Cud_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_keccak_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _s_ret_keccak_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_keccak_packed(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(pv))); }
    uint256 _res_modifier_keccak_packed;
    modifier _m_modifier_keccak_packed(U pv) { _res_modifier_keccak_packed = uint256(keccak256(abi.encodePacked(pv))); _; }
    function _e_modifier_keccak_packed(U rd) internal _m_modifier_keccak_packed(rd)  returns (uint256) { return _res_modifier_keccak_packed; }
    function _w_ptr_keccak_packed() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_keccak_packed_two() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _s_ret_keccak_packed_two() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_keccak_packed_two(U pv) internal view  returns (uint256) { return uint256(keccak256(abi.encodePacked(U.wrap(3), pv))); }
    uint256 _res_modifier_keccak_packed_two;
    modifier _m_modifier_keccak_packed_two(U pv) { _res_modifier_keccak_packed_two = uint256(keccak256(abi.encodePacked(U.wrap(3), pv))); _; }
    function _e_modifier_keccak_packed_two(U rd) internal _m_modifier_keccak_packed_two(rd)  returns (uint256) { return _res_modifier_keccak_packed_two; }
    function _w_ptr_keccak_packed_two() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(rx))); }
    function t_retdirect_keccak_packed() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(_s_retdirect_keccak_packed()))); }
    function t_ret_keccak_packed() external view  returns (uint256) { U rx = _s_ret_keccak_packed(); return uint256(keccak256(abi.encodePacked(rx))); }
    function t_param_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_keccak_packed(rx); }
    function t_copy_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = rx; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ternary_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_ifjoin_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_loopjoin_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = U.wrap(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(ry))); }
    function t_tuple_keccak_packed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } (U ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_modifier_keccak_packed() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _e_modifier_keccak_packed(rx); }
    function t_ptr_keccak_packed() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed; function() internal pure returns (U) rq; assembly { rq := rp } U ry = rq(); return uint256(keccak256(abi.encodePacked(ry))); }
    function t_mem_keccak_packed() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodePacked(ry))); }
    function t_local_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encodePacked(U.wrap(3), rx))); }
    function t_retdirect_keccak_packed_two() external view  returns (uint256) { return uint256(keccak256(abi.encodePacked(U.wrap(3), _s_retdirect_keccak_packed_two()))); }
    function t_ret_keccak_packed_two() external view  returns (uint256) { U rx = _s_ret_keccak_packed_two(); return uint256(keccak256(abi.encodePacked(U.wrap(3), rx))); }
    function t_param_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_keccak_packed_two(rx); }
    function t_copy_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = rx; return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_ternary_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry = rf ? rx : ro; return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_ifjoin_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; U ro = U.wrap(3); U ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_loopjoin_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U ry = U.wrap(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_tuple_keccak_packed_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } (U ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_modifier_keccak_packed_two() external  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _e_modifier_keccak_packed_two(rx); }
    function t_ptr_keccak_packed_two() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_keccak_packed_two; function() internal pure returns (U) rq; assembly { rq := rp } U ry = rq(); return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
    function t_mem_keccak_packed_two() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; return uint256(keccak256(abi.encodePacked(U.wrap(3), ry))); }
}
