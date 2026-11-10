//@ codegen-matrix: standard
//@ run-call: f() => 18446744073709551615

// solc returns 0; see CODEGEN-018 in docs/SOLC_DIVERGENCE.md.
contract C {
    function() external[] values;

    function target() external {}

    function f() external returns (uint64 r) {
        values.push(this.target);
        assembly {
            mstore(0, values.slot)
            sstore(keccak256(0, 32), not(0))
        }

        function() external[] memory next = new function() external[](1);
        values = next;

        assembly {
            mstore(0, values.slot)
            r := shr(192, sload(keccak256(0, 32)))
        }
    }
}
