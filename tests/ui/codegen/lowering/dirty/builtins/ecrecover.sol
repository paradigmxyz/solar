//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_retdirect_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_ret_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_param_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_copy_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_ternary_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_ifjoin_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_loopjoin_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_tuple_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_modifier_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_ptr_ecrecover_v => 721457446580647751014191829380889690493307935711
//@ run-call: Cu8_0::t_mem_ecrecover_v => 721457446580647751014191829380889690493307935711

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
    function _s_retdirect_ecrecover_v() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_ecrecover_v() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ecrecover_v(uint8 pv) internal view  returns (uint256) { return uint256(uint160(ecrecover(HASH, pv, R, S))); }
    uint256 _res_modifier_ecrecover_v;
    modifier _m_modifier_ecrecover_v(uint8 pv) { _res_modifier_ecrecover_v = uint256(uint160(ecrecover(HASH, pv, R, S))); _; }
    function _e_modifier_ecrecover_v(uint8 rd) internal _m_modifier_ecrecover_v(rd)  returns (uint256) { return _res_modifier_ecrecover_v; }
    function _w_ptr_ecrecover_v() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint160(ecrecover(HASH, rx, R, S))); }
    function t_retdirect_ecrecover_v() external view  returns (uint256) { return uint256(uint160(ecrecover(HASH, _s_retdirect_ecrecover_v(), R, S))); }
    function t_ret_ecrecover_v() external view  returns (uint256) { uint8 rx = _s_ret_ecrecover_v(); return uint256(uint160(ecrecover(HASH, rx, R, S))); }
    function t_param_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ecrecover_v(rx); }
    function t_copy_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_ternary_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_ifjoin_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_loopjoin_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_tuple_ecrecover_v() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_modifier_ecrecover_v() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_ecrecover_v(rx); }
    function t_ptr_ecrecover_v() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_ecrecover_v; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(uint160(ecrecover(HASH, ry, R, S))); }
    function t_mem_ecrecover_v() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint160(ecrecover(HASH, ry, R, S))); }
}
