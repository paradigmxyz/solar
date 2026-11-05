//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_blobhash => 0
//@ run-call: Cu8_0::t_retdirect_blobhash => 0
//@ run-call: Cu8_0::t_ret_blobhash => 0
//@ run-call: Cu8_0::t_param_blobhash => 0
//@ run-call: Cu8_0::t_copy_blobhash => 0
//@ run-call: Cu8_0::t_ternary_blobhash => 0
//@ run-call: Cu8_0::t_ifjoin_blobhash => 0
//@ run-call: Cu8_0::t_loopjoin_blobhash => 0
//@ run-call: Cu8_0::t_tuple_blobhash => 0
//@ run-call: Cu8_0::t_modifier_blobhash => 0
//@ run-call: Cu8_0::t_ptr_blobhash => 0
//@ run-call: Cu8_0::t_mem_blobhash => 0
//@ run-call: Cu64_0::t_local_blobhash => 0
//@ run-call: Cu64_0::t_retdirect_blobhash => 0
//@ run-call: Cu64_0::t_ret_blobhash => 0
//@ run-call: Cu64_0::t_param_blobhash => 0
//@ run-call: Cu64_0::t_copy_blobhash => 0
//@ run-call: Cu64_0::t_ternary_blobhash => 0
//@ run-call: Cu64_0::t_ifjoin_blobhash => 0
//@ run-call: Cu64_0::t_loopjoin_blobhash => 0
//@ run-call: Cu64_0::t_tuple_blobhash => 0
//@ run-call: Cu64_0::t_modifier_blobhash => 0
//@ run-call: Cu64_0::t_ptr_blobhash => 0
//@ run-call: Cu64_0::t_mem_blobhash => 0

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
    function _s_retdirect_blobhash() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_blobhash() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_blobhash(uint8 pv) internal view  returns (uint256) { return uint256(blobhash(pv)); }
    uint256 _res_modifier_blobhash;
    modifier _m_modifier_blobhash(uint8 pv) { _res_modifier_blobhash = uint256(blobhash(pv)); _; }
    function _e_modifier_blobhash(uint8 rd) internal _m_modifier_blobhash(rd)  returns (uint256) { return _res_modifier_blobhash; }
    function _w_ptr_blobhash() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(blobhash(rx)); }
    function t_retdirect_blobhash() external view  returns (uint256) { return uint256(blobhash(_s_retdirect_blobhash())); }
    function t_ret_blobhash() external view  returns (uint256) { uint8 rx = _s_ret_blobhash(); return uint256(blobhash(rx)); }
    function t_param_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_blobhash(rx); }
    function t_copy_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(blobhash(ry)); }
    function t_ternary_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(blobhash(ry)); }
    function t_ifjoin_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(blobhash(ry)); }
    function t_loopjoin_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(blobhash(ry)); }
    function t_tuple_blobhash() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(blobhash(ry)); }
    function t_modifier_blobhash() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_blobhash(rx); }
    function t_ptr_blobhash() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_blobhash; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(blobhash(ry)); }
    function t_mem_blobhash() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(blobhash(ry)); }
}
contract Cu64_0 {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;
    function _s_retdirect_blobhash() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_ret_blobhash() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_blobhash(uint64 pv) internal view  returns (uint256) { return uint256(blobhash(pv)); }
    uint256 _res_modifier_blobhash;
    modifier _m_modifier_blobhash(uint64 pv) { _res_modifier_blobhash = uint256(blobhash(pv)); _; }
    function _e_modifier_blobhash(uint64 rd) internal _m_modifier_blobhash(rd)  returns (uint256) { return _res_modifier_blobhash; }
    function _w_ptr_blobhash() internal pure returns (uint256) { return 0xabcd000000000000001b; }
    function t_local_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(blobhash(rx)); }
    function t_retdirect_blobhash() external view  returns (uint256) { return uint256(blobhash(_s_retdirect_blobhash())); }
    function t_ret_blobhash() external view  returns (uint256) { uint64 rx = _s_ret_blobhash(); return uint256(blobhash(rx)); }
    function t_param_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_blobhash(rx); }
    function t_copy_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = rx; return uint256(blobhash(ry)); }
    function t_ternary_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry = rf ? rx : ro; return uint256(blobhash(ry)); }
    function t_ifjoin_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bool rf = gasleft() > 0; uint64 ro = uint64(3); uint64 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(blobhash(ry)); }
    function t_loopjoin_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 ry = uint64(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(blobhash(ry)); }
    function t_tuple_blobhash() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } (uint64 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(blobhash(ry)); }
    function t_modifier_blobhash() external  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _e_modifier_blobhash(rx); }
    function t_ptr_blobhash() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_blobhash; function() internal pure returns (uint64) rq; assembly { rq := rp } uint64 ry = rq(); return uint256(blobhash(ry)); }
    function t_mem_blobhash() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(blobhash(ry)); }
}
