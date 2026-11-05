//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_0::t_retdirect_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_0::t_mem_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_0::t_local_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_0::t_retdirect_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_0::t_mem_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu8_0::t_local_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_0::t_retdirect_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_0::t_mem_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu8_0::t_local_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_0::t_retdirect_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_0::t_mem_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu8_0::t_local_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_0::t_retdirect_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_0::t_mem_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu8_0::t_local_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_0::t_retdirect_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu8_0::t_mem_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_local_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_retdirect_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_mem_cd_array_index [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_local_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_0::t_retdirect_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_0::t_mem_cd_bytes_index 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 28
//@ run-call: Cu64_0::t_local_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_0::t_retdirect_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_0::t_mem_cd_slice_len 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 13
//@ run-call: Cu64_0::t_local_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_0::t_retdirect_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_0::t_mem_cd_slice_end 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 27
//@ run-call: Cu64_0::t_local_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_0::t_retdirect_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_0::t_mem_cd_slice_both 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728 => 0
//@ run-call: Cu64_0::t_local_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_retdirect_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270
//@ run-call: Cu64_0::t_mem_cd_array_slice [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390] => 270

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
    function _s_retdirect_cd_array_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_bytes_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_len() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_end() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_slice_both() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_retdirect_cd_array_slice() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function t_local_cd_array_index(uint256[] calldata a) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return a[rx]; }
    function t_retdirect_cd_array_index(uint256[] calldata a) external view  returns (uint256) { return a[_s_retdirect_cd_array_index()]; }
    function t_mem_cd_array_index(uint256[] calldata a) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return a[ry]; }
    function t_local_cd_bytes_index(bytes calldata d) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(uint8(d[rx])); }
    function t_retdirect_cd_bytes_index(bytes calldata d) external view  returns (uint256) { return uint256(uint8(d[_s_retdirect_cd_bytes_index()])); }
    function t_mem_cd_bytes_index(bytes calldata d) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(uint8(d[ry])); }
    function t_local_cd_slice_len(bytes calldata d) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[rx:]).length; }
    function t_retdirect_cd_slice_len(bytes calldata d) external view  returns (uint256) { return bytes(d[_s_retdirect_cd_slice_len():]).length; }
    function t_mem_cd_slice_len(bytes calldata d) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[ry:]).length; }
    function t_local_cd_slice_end(bytes calldata d) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[:rx]).length; }
    function t_retdirect_cd_slice_end(bytes calldata d) external view  returns (uint256) { return bytes(d[:_s_retdirect_cd_slice_end()]).length; }
    function t_mem_cd_slice_end(bytes calldata d) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[:ry]).length; }
    function t_local_cd_slice_both(bytes calldata d) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return bytes(d[rx:rx]).length; }
    function t_retdirect_cd_slice_both(bytes calldata d) external view  returns (uint256) { return bytes(d[_s_retdirect_cd_slice_both():_s_retdirect_cd_slice_both()]).length; }
    function t_mem_cd_slice_both(bytes calldata d) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return bytes(d[ry:ry]).length; }
    function t_local_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return a[rx:][0]; }
    function t_retdirect_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { return a[_s_retdirect_cd_array_slice():][0]; }
    function t_mem_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return a[ry:][0]; }
}
contract Cu64_0 {
    function _s_retdirect_cd_array_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_bytes_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_len() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_end() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_slice_both() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _s_retdirect_cd_array_slice() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function t_local_cd_array_index(uint256[] calldata a) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return a[rx]; }
    function t_retdirect_cd_array_index(uint256[] calldata a) external view  returns (uint256) { return a[_s_retdirect_cd_array_index()]; }
    function t_mem_cd_array_index(uint256[] calldata a) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return a[ry]; }
    function t_local_cd_bytes_index(bytes calldata d) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(uint8(d[rx])); }
    function t_retdirect_cd_bytes_index(bytes calldata d) external view  returns (uint256) { return uint256(uint8(d[_s_retdirect_cd_bytes_index()])); }
    function t_mem_cd_bytes_index(bytes calldata d) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(uint8(d[ry])); }
    function t_local_cd_slice_len(bytes calldata d) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[rx:]).length; }
    function t_retdirect_cd_slice_len(bytes calldata d) external view  returns (uint256) { return bytes(d[_s_retdirect_cd_slice_len():]).length; }
    function t_mem_cd_slice_len(bytes calldata d) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[ry:]).length; }
    function t_local_cd_slice_end(bytes calldata d) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[:rx]).length; }
    function t_retdirect_cd_slice_end(bytes calldata d) external view  returns (uint256) { return bytes(d[:_s_retdirect_cd_slice_end()]).length; }
    function t_mem_cd_slice_end(bytes calldata d) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[:ry]).length; }
    function t_local_cd_slice_both(bytes calldata d) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return bytes(d[rx:rx]).length; }
    function t_retdirect_cd_slice_both(bytes calldata d) external view  returns (uint256) { return bytes(d[_s_retdirect_cd_slice_both():_s_retdirect_cd_slice_both()]).length; }
    function t_mem_cd_slice_both(bytes calldata d) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return bytes(d[ry:ry]).length; }
    function t_local_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return a[rx:][0]; }
    function t_retdirect_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { return a[_s_retdirect_cd_array_slice():][0]; }
    function t_mem_cd_array_slice(uint256[] calldata a) external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return a[ry:][0]; }
}
