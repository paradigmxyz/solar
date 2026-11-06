//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: Cb1_0::t_local_error_revert => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_retdirect_error_revert => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_param_error_revert => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_mem_error_revert => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_local_error_require => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_retdirect_error_require => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_param_error_require => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_mem_error_require => 0x5afef2afab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_local_error_two => 0x70a8fdc3cd00000000000000000000000000000000000000000000000000000000000000ab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_retdirect_error_two => 0x70a8fdc3cd00000000000000000000000000000000000000000000000000000000000000ab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_param_error_two => 0x70a8fdc3cd00000000000000000000000000000000000000000000000000000000000000ab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_mem_error_two => 0x70a8fdc3cd00000000000000000000000000000000000000000000000000000000000000ab00000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_local_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000276ab000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_retdirect_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000276ab000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_param_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000276ab000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb1_0::t_mem_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000276ab000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_local_error_revert => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_retdirect_error_revert => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_param_error_revert => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_mem_error_revert => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_local_error_require => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_retdirect_error_require => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_param_error_require => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_mem_error_require => 0x99845128deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_local_error_two => 0x66498bb21122334400000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_retdirect_error_two => 0x66498bb21122334400000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_param_error_two => 0x66498bb21122334400000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_mem_error_two => 0x66498bb21122334400000000000000000000000000000000000000000000000000000000deadbeef00000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_local_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000576deadbeef000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_retdirect_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000576deadbeef000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_param_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000576deadbeef000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb4_0::t_mem_revert_string_concat => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000576deadbeef000000000000000000000000000000000000000000000000000000
//@ run-call-fail: Cb20_0::t_local_error_revert => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_retdirect_error_revert => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_param_error_revert => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_mem_error_revert => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_local_error_require => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_retdirect_error_require => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_param_error_require => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_mem_error_require => 0x39d052e71234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_local_error_two => 0x0467479711111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_retdirect_error_two => 0x0467479711111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_param_error_two => 0x0467479711111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_mem_error_two => 0x0467479711111111111111111111111111111111111111110000000000000000000000001234567890123456789012345678901234567890000000000000000000000000
//@ run-call-fail: Cb20_0::t_local_revert_string_concat => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000157612345678901234567890123456789012345678900000000000000000000000
//@ run-call-fail: Cb20_0::t_retdirect_revert_string_concat => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000157612345678901234567890123456789012345678900000000000000000000000
//@ run-call-fail: Cb20_0::t_param_revert_string_concat => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000157612345678901234567890123456789012345678900000000000000000000000
//@ run-call-fail: Cb20_0::t_mem_revert_string_concat => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000157612345678901234567890123456789012345678900000000000000000000000

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

error Err_b1(bytes1 v);
error Err2_b1(bytes1 a, bytes1 b);

error Err_b4(bytes4 v);
error Err2_b4(bytes4 a, bytes4 b);

error Err_b20(bytes20 v);
error Err2_b20(bytes20 a, bytes20 b);

contract Cb1_0 {
    function _s_retdirect_error_revert() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_error_revert(bytes1 pv) internal view  returns (uint256) { revert Err_b1(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_error_require(bytes1 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_b1(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_error_two(bytes1 pv) internal view  returns (uint256) { revert Err2_b1(bytes1(0xcd), pv); return 0; }
    function _s_retdirect_revert_string_concat() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_revert_string_concat(bytes1 pv) internal view  returns (uint256) { revert(string(abi.encodePacked("v", pv))); return 0; }
    function t_local_error_revert() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } revert Err_b1(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_b1(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; revert Err_b1(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } require(gasleft() == 0, Err_b1(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_b1(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; require(gasleft() == 0, Err_b1(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } revert Err2_b1(bytes1(0xcd), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_b1(bytes1(0xcd), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; revert Err2_b1(bytes1(0xcd), ry); return 0; }
    function t_local_revert_string_concat() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } revert(string(abi.encodePacked("v", rx))); return 0; }
    function t_retdirect_revert_string_concat() external view  returns (uint256) { revert(string(abi.encodePacked("v", _s_retdirect_revert_string_concat()))); return 0; }
    function t_param_revert_string_concat() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_revert_string_concat(rx); }
    function t_mem_revert_string_concat() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; revert(string(abi.encodePacked("v", ry))); return 0; }
}
contract Cb4_0 {
    function _s_retdirect_error_revert() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_error_revert(bytes4 pv) internal view  returns (uint256) { revert Err_b4(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_error_require(bytes4 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_b4(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_error_two(bytes4 pv) internal view  returns (uint256) { revert Err2_b4(bytes4(0x11223344), pv); return 0; }
    function _s_retdirect_revert_string_concat() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_revert_string_concat(bytes4 pv) internal view  returns (uint256) { revert(string(abi.encodePacked("v", pv))); return 0; }
    function t_local_error_revert() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } revert Err_b4(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_b4(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; revert Err_b4(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } require(gasleft() == 0, Err_b4(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_b4(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; require(gasleft() == 0, Err_b4(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } revert Err2_b4(bytes4(0x11223344), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_b4(bytes4(0x11223344), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; revert Err2_b4(bytes4(0x11223344), ry); return 0; }
    function t_local_revert_string_concat() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } revert(string(abi.encodePacked("v", rx))); return 0; }
    function t_retdirect_revert_string_concat() external view  returns (uint256) { revert(string(abi.encodePacked("v", _s_retdirect_revert_string_concat()))); return 0; }
    function t_param_revert_string_concat() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_revert_string_concat(rx); }
    function t_mem_revert_string_concat() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; revert(string(abi.encodePacked("v", ry))); return 0; }
}
contract Cb20_0 {
    function _s_retdirect_error_revert() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_error_revert(bytes20 pv) internal view  returns (uint256) { revert Err_b20(pv); return 0; }
    function _s_retdirect_error_require() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_error_require(bytes20 pv) internal view  returns (uint256) { require(gasleft() == 0, Err_b20(pv)); return 0; }
    function _s_retdirect_error_two() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_error_two(bytes20 pv) internal view  returns (uint256) { revert Err2_b20(bytes20(0x1111111111111111111111111111111111111111), pv); return 0; }
    function _s_retdirect_revert_string_concat() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_revert_string_concat(bytes20 pv) internal view  returns (uint256) { revert(string(abi.encodePacked("v", pv))); return 0; }
    function t_local_error_revert() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } revert Err_b20(rx); return 0; }
    function t_retdirect_error_revert() external view  returns (uint256) { revert Err_b20(_s_retdirect_error_revert()); return 0; }
    function t_param_error_revert() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_error_revert(rx); }
    function t_mem_error_revert() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; revert Err_b20(ry); return 0; }
    function t_local_error_require() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } require(gasleft() == 0, Err_b20(rx)); return 0; }
    function t_retdirect_error_require() external view  returns (uint256) { require(gasleft() == 0, Err_b20(_s_retdirect_error_require())); return 0; }
    function t_param_error_require() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_error_require(rx); }
    function t_mem_error_require() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; require(gasleft() == 0, Err_b20(ry)); return 0; }
    function t_local_error_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } revert Err2_b20(bytes20(0x1111111111111111111111111111111111111111), rx); return 0; }
    function t_retdirect_error_two() external view  returns (uint256) { revert Err2_b20(bytes20(0x1111111111111111111111111111111111111111), _s_retdirect_error_two()); return 0; }
    function t_param_error_two() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_error_two(rx); }
    function t_mem_error_two() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; revert Err2_b20(bytes20(0x1111111111111111111111111111111111111111), ry); return 0; }
    function t_local_revert_string_concat() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } revert(string(abi.encodePacked("v", rx))); return 0; }
    function t_retdirect_revert_string_concat() external view  returns (uint256) { revert(string(abi.encodePacked("v", _s_retdirect_revert_string_concat()))); return 0; }
    function t_param_revert_string_concat() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_revert_string_concat(rx); }
    function t_mem_revert_string_concat() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; revert(string(abi.encodePacked("v", ry))); return 0; }
}
