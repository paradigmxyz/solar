// ported-from: test/libsolidity/syntaxTests/array/calldata_multi_dynamic_V1.sol
pragma abicoder v1;
contract Test {
    function f(uint[][] calldata) external { } //~ ERROR: this type is only supported in ABI coder v2
    function g(uint[][1] calldata) external { } //~ ERROR: this type is only supported in ABI coder v2
}
