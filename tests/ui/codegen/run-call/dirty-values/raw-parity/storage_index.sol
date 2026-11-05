//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: C::t_storage_index_or => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: C::t_storage_index_xor => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: C::t_storage_index_and => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: C::t_bytes_index_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: C::t_bytes_index_write => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: C::t_bytes_index_clean => 2
//@ run-call: C::t_storage_index_clean => 2
//@ run-call-fail: C::t_storage_index_postfix => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: C::t_storage_index_ternary => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// the storage-array index stays raw, also through bitwise expressions.

contract C {
    uint256[] sarr;
    bytes by = hex"010203";

    function t_storage_index_or() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x100 } return sarr[x | 1]; }
    function t_storage_index_xor() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return sarr[x ^ 0]; }
    function t_storage_index_and() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return sarr[x & x]; }
    function t_bytes_index_read() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return uint256(uint8(by[x])); }
    function t_bytes_index_write() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } by[x] = 0x05; return 1; }
    function t_bytes_index_clean() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return uint256(uint8(by[x & 0xff])); }
    function t_storage_index_clean() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return sarr[x & 0xff]; }
    function t_storage_index_postfix() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return sarr[x++]; }
    function t_storage_index_ternary() external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); uint8 x; assembly { x := 0x101 } return sarr[gasleft() > 0 ? x : x]; }
}
