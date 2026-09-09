//@ codegen-matrix: standard
//@ run-call: first 321 => 321
//@ run-call: second 321 => 321
//@ run-call: third 321 => 321
//@ run-call: masked 321 => 65
//@ run-call: first 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255

contract SharedTerminal {
    function first(uint256 value) external pure returns (uint256) { return value; }
    function second(uint256 value) external pure returns (uint256) { return value; }
    function third(uint256 value) external pure returns (uint256) { return value; }
    function masked(uint256 value) external pure returns (uint256) { return value & 255; }
}
