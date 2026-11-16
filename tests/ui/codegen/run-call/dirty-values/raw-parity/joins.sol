//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_ternary_asm_read => 11259163
//@ run-call: Cu8_0::t_ifjoin_asm_read => 11259163
//@ run-call: Cu8_0::t_loopjoin_asm_read => 11259163
//@ run-call: Cu8_0::t_ternary_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ifjoin_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_loopjoin_asm_read_typed_also => 11259164
//@ run-call: Caddr_0::t_ternary_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ifjoin_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_loopjoin_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ternary_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ifjoin_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_loopjoin_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cb4_0::t_ternary_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ifjoin_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_loopjoin_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ternary_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ifjoin_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_loopjoin_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cen_0::t_ternary_asm_read => 257
//@ run-call: Cen_0::t_ifjoin_asm_read => 257
//@ run-call: Cen_0::t_loopjoin_asm_read => 257
//@ run-call-fail: Cen_0::t_ternary_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ifjoin_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_loopjoin_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: dirt surviving copies, calls, returns and joins into an assembly read.

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
    function t_ternary_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddr_0 {
    function t_ternary_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } address t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } address t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry = rf ? rx : ro; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } bool rf = gasleft() > 0; address ro = address(0x1111111111111111111111111111111111111111); address ry; if (rf) { ry = rx; } else { ry = ro; } address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = address(0x1111111111111111111111111111111111111111); for (uint256 ri; ri < 1; ++ri) { ry = rx; } address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb4_0 {
    function t_ternary_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry = rf ? rx : ro; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bool rf = gasleft() > 0; bytes4 ro = bytes4(0x11223344); bytes4 ry; if (rf) { ry = rx; } else { ry = ro; } bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = bytes4(0x11223344); for (uint256 ri; ri < 1; ++ri) { ry = rx; } bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cen_0 {
    function t_ternary_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_ifjoin_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } E t = ry; uint256 r; assembly { r := t } return r; }
    function t_loopjoin_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } E t = ry; uint256 r; assembly { r := t } return r; }
    function t_ternary_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry = rf ? rx : ro; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ifjoin_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } bool rf = gasleft() > 0; E ro = E.A; E ry; if (rf) { ry = rx; } else { ry = ro; } E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_loopjoin_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = E.A; for (uint256 ri; ri < 1; ++ri) { ry = rx; } E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
