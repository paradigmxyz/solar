//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Cbool_0::t_local_error_revert => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_retdirect_error_revert => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_param_error_revert => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_mem_error_revert => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_local_error_require => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_retdirect_error_require => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_param_error_require => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_mem_error_require => 0x5bc03f320000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_local_error_two => 0xf5a92c2500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_retdirect_error_two => 0xf5a92c2500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_param_error_two => 0xf5a92c2500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_mem_error_two => 0xf5a92c2500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_local_error_struct => 0xe4acc38f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_retdirect_error_struct => 0xe4acc38f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_param_error_struct => 0xe4acc38f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cbool_0::t_mem_error_struct => 0xe4acc38f00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: Cen_0::t_local_error_revert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_error_revert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_error_revert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_error_revert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_error_require => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_error_require => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_error_require => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_error_require => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_error_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_error_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_error_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_error_two => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_error_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_error_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_error_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_error_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cud_0::t_local_error_revert => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_retdirect_error_revert => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_param_error_revert => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_mem_error_revert => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_local_error_require => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_retdirect_error_require => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_param_error_require => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_mem_error_require => 0x89ae4f6d000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_local_error_two => 0xfd868b670000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_retdirect_error_two => 0xfd868b670000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_param_error_two => 0xfd868b670000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_mem_error_two => 0xfd868b670000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_local_error_struct => 0x05eb84980000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_retdirect_error_struct => 0x05eb84980000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_param_error_struct => 0x05eb84980000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cud_0::t_mem_error_struct => 0x05eb84980000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: ABI encodings and custom errors.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

struct St_bool {
    uint256 a;
    bool f;
}

error Err_bool(bool v);
error Err2_bool(bool a, bool b);

error ErrS_bool(St_bool s);

struct St_en {
    uint256 a;
    E f;
}

error Err_en(E v);
error Err2_en(E a, E b);

error ErrS_en(St_en s);

struct St_ud {
    uint256 a;
    U f;
}

error Err_ud(U v);
error Err2_ud(U a, U b);

error ErrS_ud(St_ud s);

contract Cbool_0 {
    function _s_retdirect_error_revert() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_error_revert(bool pv) internal view  returns (uint256) { revert Err_bool(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_error_require(bool pv) internal view  returns (uint256) { require(gasleft() == 0, Err_bool(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_error_two(bool pv) internal view  returns (uint256) { revert Err2_bool(false, pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_error_struct(bool pv) internal view  returns (uint256) { revert ErrS_bool(St_bool(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } revert Err_bool(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_bool(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; revert Err_bool(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } require(gasleft() == 0, Err_bool(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_bool(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; require(gasleft() == 0, Err_bool(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } revert Err2_bool(false, rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_bool(false, _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; revert Err2_bool(false, ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } revert ErrS_bool(St_bool(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_bool(St_bool(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; revert ErrS_bool(St_bool(1, ry)); return 0; }
}
contract Cen_0 {
    function _s_retdirect_error_revert() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_error_revert(E pv) internal view  returns (uint256) { revert Err_en(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_error_require(E pv) internal view  returns (uint256) { require(gasleft() == 0, Err_en(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_error_two(E pv) internal view  returns (uint256) { revert Err2_en(E.A, pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_error_struct(E pv) internal view  returns (uint256) { revert ErrS_en(St_en(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { E rx; assembly { rx := 0x101 } revert Err_en(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_en(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; revert Err_en(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { E rx; assembly { rx := 0x101 } require(gasleft() == 0, Err_en(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_en(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; require(gasleft() == 0, Err_en(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } revert Err2_en(E.A, rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_en(E.A, _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; revert Err2_en(E.A, ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { E rx; assembly { rx := 0x101 } revert ErrS_en(St_en(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_en(St_en(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; revert ErrS_en(St_en(1, ry)); return 0; }
}
contract Cud_0 {
    function _s_retdirect_error_revert() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_error_revert(U pv) internal view  returns (uint256) { revert Err_ud(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_error_require(U pv) internal view  returns (uint256) { require(gasleft() == 0, Err_ud(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_error_two(U pv) internal view  returns (uint256) { revert Err2_ud(U.wrap(3), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_error_struct(U pv) internal view  returns (uint256) { revert ErrS_ud(St_ud(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } revert Err_ud(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_ud(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; revert Err_ud(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } require(gasleft() == 0, Err_ud(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_ud(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; require(gasleft() == 0, Err_ud(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } revert Err2_ud(U.wrap(3), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_ud(U.wrap(3), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; revert Err2_ud(U.wrap(3), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } revert ErrS_ud(St_ud(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_ud(St_ud(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; revert ErrS_ud(St_ud(1, ry)); return 0; }
}
