contract YulOps {
    function op_add(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := add(a, b) } }
    function op_sub(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := sub(a, b) } }
    function op_mul(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := mul(a, b) } }
    function op_div(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := div(a, b) } }
    function op_sdiv(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := sdiv(a, b) } }
    function op_mod(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := mod(a, b) } }
    function op_smod(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := smod(a, b) } }
    function op_exp(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := exp(a, b) } }
    function op_not(uint256 a) external pure returns (uint256 r) { assembly { r := not(a) } }
    function op_lt(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := lt(a, b) } }
    function op_gt(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := gt(a, b) } }
    function op_slt(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := slt(a, b) } }
    function op_sgt(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := sgt(a, b) } }
    function op_eq(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := eq(a, b) } }
    function op_iszero(uint256 a) external pure returns (uint256 r) { assembly { r := iszero(a) } }
    function op_and(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := and(a, b) } }
    function op_or(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := or(a, b) } }
    function op_xor(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := xor(a, b) } }
    function op_byte(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := byte(a, b) } }
    function op_shl(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := shl(a, b) } }
    function op_shr(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := shr(a, b) } }
    function op_sar(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := sar(a, b) } }
    function op_addmod(uint256 a, uint256 b, uint256 c) external pure returns (uint256 r) { assembly { r := addmod(a, b, c) } }
    function op_mulmod(uint256 a, uint256 b, uint256 c) external pure returns (uint256 r) { assembly { r := mulmod(a, b, c) } }
    function op_signextend(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { r := signextend(a, b) } }
    function op_keccak(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { mstore(0, a) mstore(32, b) r := keccak256(0, 64) } }
    function op_keccak_len(uint256 a, uint256 n) external pure returns (uint256 r) { assembly { mstore(0, a) r := keccak256(0, and(n, 63)) } }
    function op_mload_mstore8(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { mstore(0, a) mstore8(31, b) r := mload(0) } }
    function op_calldataload(uint256 a) external pure returns (uint256 r) { assembly { r := calldataload(and(a, 255)) } }
    function op_calldatasize() external pure returns (uint256 r) { assembly { r := calldatasize() } }
    function op_calldatacopy(uint256 a, uint256 n) external pure returns (uint256 r) { assembly { calldatacopy(0, and(a, 127), and(n, 63)) r := mload(0) } }
    function op_mcopy(uint256 a, uint256 b) external pure returns (uint256 r) { assembly { mstore(0, a) mstore(32, b) mcopy(64, 0, 64) mcopy(80, 64, 32) r := mload(96) } }
    function op_selfbalance() external view returns (uint256 r) { assembly { r := selfbalance() } }
    function op_chainid() external view returns (uint256 r) { assembly { r := chainid() } }
    function op_basefee() external view returns (uint256 r) { assembly { r := basefee() } }
    function op_number() external view returns (uint256 r) { assembly { r := number() } }
    function op_timestamp() external view returns (uint256 r) { assembly { r := timestamp() } }
    function op_caller() external view returns (uint256 r) { assembly { r := caller() } }
    function op_origin() external view returns (uint256 r) { assembly { r := origin() } }
    function op_callvalue() external view returns (uint256 r) { assembly { r := callvalue() } }
    function op_gasprice() external view returns (uint256 r) { assembly { r := gasprice() } }
    function op_coinbase() external view returns (uint256 r) { assembly { r := coinbase() } }
    function op_gaslimit() external view returns (uint256 r) { assembly { r := gaslimit() } }
    function op_prevrandao() external view returns (uint256 r) { assembly { r := prevrandao() } }
    function op_blobbasefee() external view returns (uint256 r) { assembly { r := blobbasefee() } }
    function op_returndatasize() external pure returns (uint256 r) { assembly { r := returndatasize() } }
    function op_tstore(uint256 a, uint256 b) external returns (uint256 r) { assembly { tstore(a, b) r := tload(a) } }
    function op_tload_zero(uint256 a) external view returns (uint256 r) { assembly { r := tload(a) } }
    function op_sstore(uint256 a, uint256 b) external returns (uint256 r) { assembly { sstore(a, b) r := sload(a) } }
    function ctl_switch(uint256 a) external pure returns (uint256 r) { assembly { switch a case 0 { r := 10 } case 1 { r := 11 } case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff { r := 12 } default { r := 99 } } }
    function ctl_switch_nodefault(uint256 a) external pure returns (uint256 r) { assembly { r := 5 switch a case 0 { r := 10 } case 2 { r := 12 } } }
    function ctl_for(uint256 a) external pure returns (uint256 r) { assembly { for { let i := 0 } lt(i, and(a, 15)) { i := add(i, 1) } { if eq(i, 7) { continue } if eq(i, 12) { break } r := add(r, i) } } }
    function ctl_fn(uint256 a) external pure returns (uint256 r) { assembly { function f(x) -> y { if gt(x, 10) { y := 1 leave } y := add(x, 1) } r := f(a) } }
    function ctl_fn_multi(uint256 a, uint256 b) external pure returns (uint256 r, uint256 s) { assembly { function f(x, y) -> p, q { p := add(x, y) q := sub(x, y) } r, s := f(a, b) } }
    function ctl_nested(uint256 a) external pure returns (uint256 r) { assembly { let x := a { let x_ := 5 r := add(x, x_) } { let y := 7 r := add(r, y) } } }
    function ctl_recursion(uint256 a) external pure returns (uint256 r) { assembly { function fact(n) -> f { switch n case 0 { f := 1 } default { f := mul(n, fact(sub(n, 1))) } } r := fact(and(a, 7)) } }
    function ctl_revert(uint256 a) external pure returns (uint256 r) { assembly { if eq(a, 1) { mstore(0, 0xdead) revert(0, 32) } if eq(a, 2) { invalid() } if eq(a, 3) { mstore(0, 7) return(0, 32) } if eq(a, 4) { stop() } r := 42 } }
    function ctl_pop(uint256 a) external pure returns (uint256 r) { assembly { pop(add(a, 1)) r := a } }
    function ctl_solidity_var(uint256 a) external pure returns (uint256 r) { uint256 x = a; assembly { x := add(x, 1) } r = x * 2; assembly { r := add(r, x) } }
    function ctl_memory_var(uint256 a) external pure returns (uint256 r) { uint256[] memory m = new uint256[](2); assembly { mstore(add(m, 32), a) } r = m[0]; }
    function ctl_storage_slot(uint256 a) external returns (uint256 r) { assembly { sstore(s.slot, a) } r = s; }
    uint256 s;
    struct P { uint8 a; uint8 b; }
    P p;
    function ctl_storage_offset(uint256 a) external returns (uint256 r) { assembly { sstore(p.slot, a) } r = uint256(p.b) * 256 + p.a; assembly { r := add(r, mul(p.offset, 0x10000)) } }
    function ctl_calldata_len(bytes calldata d) external pure returns (uint256 r) { assembly { r := add(d.offset, mul(d.length, 0x10000)) } }
    function ctl_string_lit() external pure returns (uint256 r) { assembly { r := "abc" } }
    function ctl_hex_lit() external pure returns (uint256 r) { assembly { r := 0x1234 } }
    function ctl_true() external pure returns (uint256 r) { assembly { r := true } }
    function ctl_bool_ret(uint256 a) external pure returns (bool r) { assembly { r := a } }
}
