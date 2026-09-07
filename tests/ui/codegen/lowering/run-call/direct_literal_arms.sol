//@ codegen-matrix: standard
//@ run-call: pick 0, 0 => 0
//@ run-call: pick 1, 2 => 4
//@ run-call: pick 2, 1 => 4
//@ run-call: pick 3, 3 => 12
//@ run-call: pick 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: pick 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: pick 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: truthy 0 => 1
//@ run-call: truthy 1 => 0
//@ run-call: truthy 2 => 0
//@ run-call: truthy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: descending 0 => 4
//@ run-call: descending 2 => 36

// The checked arithmetic body is identical to the original spill sharing fixture.
// Runtime oracles require exact truthiness and overflow behavior; physical activation
// is established separately by the EVM IR fixtures and captured lowering output.
contract DirectLiteralArms {
    function pick(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 c = a * b;
        if (a > b) {
            return c + a;
        }
        return c + b;
    }
    function truthy(uint256 condition) external pure returns (uint256 value) {
        assembly {
            value := 1
            if condition { value := 0 }
        }
    }
    function descending(uint256 condition) external pure returns (uint256 value) {
        assembly {
            value := 4
            if condition { value := 36 }
        }
    }
}
