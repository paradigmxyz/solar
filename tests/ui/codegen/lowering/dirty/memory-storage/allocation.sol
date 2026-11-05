//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_new_array_len => 27
//@ run-call: Cu8_0::t_retdirect_new_array_len => 27
//@ run-call: Cu8_0::t_param_new_array_len => 27
//@ run-call: Cu8_0::t_mem_new_array_len => 27
//@ run-call: Cu8_0::t_local_new_bytes_len => 27
//@ run-call: Cu8_0::t_retdirect_new_bytes_len => 27
//@ run-call: Cu8_0::t_param_new_bytes_len => 27
//@ run-call: Cu8_0::t_mem_new_bytes_len => 27
//@ run-call: Cu64_0::t_local_new_array_len => 27
//@ run-call: Cu64_0::t_retdirect_new_array_len => 27
//@ run-call: Cu64_0::t_param_new_array_len => 27
//@ run-call: Cu64_0::t_mem_new_array_len => 27
//@ run-call: Cu64_0::t_local_new_bytes_len => 27
//@ run-call: Cu64_0::t_retdirect_new_bytes_len => 27
//@ run-call: Cu64_0::t_param_new_bytes_len => 27
//@ run-call: Cu64_0::t_mem_new_bytes_len => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: memory and storage stores, keys, indexes, slices and allocations.

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
    function _s_retdirect_new_array_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_new_array_len(uint8 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](pv); return a.length; }
    function _s_retdirect_new_bytes_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_new_bytes_len(uint8 pv) internal view  returns (uint256) { bytes memory bb = new bytes(pv); return bb.length; }
    function t_local_new_array_len() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](rx); return a.length; }
    function t_retdirect_new_array_len() external view  returns (uint256) { uint256[] memory a = new uint256[](_s_retdirect_new_array_len()); return a.length; }
    function t_param_new_array_len() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_new_array_len(rx); }
    function t_mem_new_array_len() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](ry); return a.length; }
    function t_local_new_bytes_len() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes memory bb = new bytes(rx); return bb.length; }
    function t_retdirect_new_bytes_len() external view  returns (uint256) { bytes memory bb = new bytes(_s_retdirect_new_bytes_len()); return bb.length; }
    function t_param_new_bytes_len() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_new_bytes_len(rx); }
    function t_mem_new_bytes_len() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes memory bb = new bytes(ry); return bb.length; }
}
contract Cu64_0 {
    function _s_retdirect_new_array_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_new_array_len(uint64 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](pv); return a.length; }
    function _s_retdirect_new_bytes_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_new_bytes_len(uint64 pv) internal view  returns (uint256) { bytes memory bb = new bytes(pv); return bb.length; }
    function t_local_new_array_len() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](rx); return a.length; }
    function t_retdirect_new_array_len() external view  returns (uint256) { uint256[] memory a = new uint256[](_s_retdirect_new_array_len()); return a.length; }
    function t_param_new_array_len() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_new_array_len(rx); }
    function t_mem_new_array_len() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](ry); return a.length; }
    function t_local_new_bytes_len() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes memory bb = new bytes(rx); return bb.length; }
    function t_retdirect_new_bytes_len() external view  returns (uint256) { bytes memory bb = new bytes(_s_retdirect_new_bytes_len()); return bb.length; }
    function t_param_new_bytes_len() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_new_bytes_len(rx); }
    function t_mem_new_bytes_len() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes memory bb = new bytes(ry); return bb.length; }
}
