//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Cu8_0::t_local_error_revert => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_retdirect_error_revert => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_param_error_revert => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_mem_error_revert => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_local_error_require => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_retdirect_error_require => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_param_error_require => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_mem_error_require => 0xf5555419000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_local_error_two => 0x9123a0f20000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_retdirect_error_two => 0x9123a0f20000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_param_error_two => 0x9123a0f20000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_mem_error_two => 0x9123a0f20000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_local_error_struct => 0x214828940000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_retdirect_error_struct => 0x214828940000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_param_error_struct => 0x214828940000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu8_0::t_mem_error_struct => 0x214828940000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_local_error_revert => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_retdirect_error_revert => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_param_error_revert => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_mem_error_revert => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_local_error_require => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_retdirect_error_require => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_param_error_require => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_mem_error_require => 0xdc65aa39000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_local_error_two => 0xc23c9af60000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_retdirect_error_two => 0xc23c9af60000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_param_error_two => 0xc23c9af60000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_mem_error_two => 0xc23c9af60000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_local_error_struct => 0xb0b920f40000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_retdirect_error_struct => 0xb0b920f40000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_param_error_struct => 0xb0b920f40000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Cu64_0::t_mem_error_struct => 0xb0b920f40000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001b
//@ run-call-fail: Ci8_0::t_local_error_revert => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_retdirect_error_revert => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_param_error_revert => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_mem_error_revert => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_local_error_require => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_retdirect_error_require => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_param_error_require => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_mem_error_require => 0xd4e54b32fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_local_error_two => 0x7bdcb09cfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_retdirect_error_two => 0x7bdcb09cfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_param_error_two => 0x7bdcb09cfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_mem_error_two => 0x7bdcb09cfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_local_error_struct => 0xe8a0b74d0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_retdirect_error_struct => 0xe8a0b74d0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_param_error_struct => 0xe8a0b74d0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci8_0::t_mem_error_struct => 0xe8a0b74d0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_local_error_revert => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_retdirect_error_revert => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_param_error_revert => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_mem_error_revert => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_local_error_require => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_retdirect_error_require => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_param_error_require => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_mem_error_require => 0xa3758777fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_local_error_two => 0xc5039825fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_retdirect_error_two => 0xc5039825fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_param_error_two => 0xc5039825fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_mem_error_two => 0xc5039825fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_local_error_struct => 0x636e06ba0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_retdirect_error_struct => 0x636e06ba0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_param_error_struct => 0x636e06ba0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: Ci24_0::t_mem_error_struct => 0x636e06ba0000000000000000000000000000000000000000000000000000000000000001fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd

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

struct St_u8 {
    uint256 a;
    uint8 f;
}

error Err_u8(uint8 v);
error Err2_u8(uint8 a, uint8 b);

error ErrS_u8(St_u8 s);

struct St_u64 {
    uint256 a;
    uint64 f;
}

error Err_u64(uint64 v);
error Err2_u64(uint64 a, uint64 b);

error ErrS_u64(St_u64 s);

struct St_i8 {
    uint256 a;
    int8 f;
}

error Err_i8(int8 v);
error Err2_i8(int8 a, int8 b);

error ErrS_i8(St_i8 s);

struct St_i24 {
    uint256 a;
    int24 f;
}

error Err_i24(int24 v);
error Err2_i24(int24 a, int24 b);

error ErrS_i24(St_i24 s);

contract Cu8_0 {
    function _s_retdirect_error_revert() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_error_revert(uint8 pv) internal view  returns (uint256) { revert Err_u8(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_error_require(uint8 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_u8(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_error_two(uint8 pv) internal view  returns (uint256) { revert Err2_u8(uint8(3), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_error_struct(uint8 pv) internal view  returns (uint256) { revert ErrS_u8(St_u8(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } revert Err_u8(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_u8(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; revert Err_u8(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } require(gasleft() == 0, Err_u8(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_u8(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; require(gasleft() == 0, Err_u8(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } revert Err2_u8(uint8(3), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_u8(uint8(3), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; revert Err2_u8(uint8(3), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } revert ErrS_u8(St_u8(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_u8(St_u8(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; revert ErrS_u8(St_u8(1, ry)); return 0; }
}
contract Cu64_0 {
    function _s_retdirect_error_revert() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_error_revert(uint64 pv) internal view  returns (uint256) { revert Err_u64(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_error_require(uint64 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_u64(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_error_two(uint64 pv) internal view  returns (uint256) { revert Err2_u64(uint64(3), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_error_struct(uint64 pv) internal view  returns (uint256) { revert ErrS_u64(St_u64(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } revert Err_u64(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_u64(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; revert Err_u64(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } require(gasleft() == 0, Err_u64(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_u64(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; require(gasleft() == 0, Err_u64(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } revert Err2_u64(uint64(3), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_u64(uint64(3), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; revert Err2_u64(uint64(3), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } revert ErrS_u64(St_u64(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_u64(St_u64(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; revert ErrS_u64(St_u64(1, ry)); return 0; }
}
contract Ci8_0 {
    function _s_retdirect_error_revert() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_error_revert(int8 pv) internal view  returns (uint256) { revert Err_i8(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_error_require(int8 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_i8(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_error_two(int8 pv) internal view  returns (uint256) { revert Err2_i8(int8(-2), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_error_struct(int8 pv) internal view  returns (uint256) { revert ErrS_i8(St_i8(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } revert Err_i8(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_i8(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; revert Err_i8(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } require(gasleft() == 0, Err_i8(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_i8(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; require(gasleft() == 0, Err_i8(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } revert Err2_i8(int8(-2), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_i8(int8(-2), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; revert Err2_i8(int8(-2), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } revert ErrS_i8(St_i8(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_i8(St_i8(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; revert ErrS_i8(St_i8(1, ry)); return 0; }
}
contract Ci24_0 {
    function _s_retdirect_error_revert() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_error_revert(int24 pv) internal view  returns (uint256) { revert Err_i24(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_error_require(int24 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_i24(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_error_two(int24 pv) internal view  returns (uint256) { revert Err2_i24(int24(-2), pv); return 0; }
    function _s_retdirect_error_struct() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_error_struct(int24 pv) internal view  returns (uint256) { revert ErrS_i24(St_i24(1, pv)); return 0; }
    function t_local_error_revert() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } revert Err_i24(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_i24(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; revert Err_i24(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } require(gasleft() == 0, Err_i24(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_i24(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; require(gasleft() == 0, Err_i24(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } revert Err2_i24(int24(-2), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_i24(int24(-2), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; revert Err2_i24(int24(-2), ry); return 0; }
    function t_local_error_struct() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } revert ErrS_i24(St_i24(1, rx)); return 0; }
    function t_retdirect_error_struct() external view  returns (uint256) { revert ErrS_i24(St_i24(1, _s_retdirect_error_struct())); return 0; }
    function t_param_error_struct() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_error_struct(rx); }
    function t_mem_error_struct() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; revert ErrS_i24(St_i24(1, ry)); return 0; }
}
