//@ codegen-matrix: standard
//@ run-call: fill 3 => 96
//@ run-call: fill 0 => 0
//@ run-call: viaCallee 3 => 5
//@ run-call: modulo 7 => 128

// A store at a runtime index into memory the block allocates, in the block or in a callee,
// stays inside that memory, so it does not store a reference outside the block.
contract Test {
    function fill(uint256 n) public pure returns (uint256 total) {
        /// @custom:solar-scratch
        {
            bytes[] memory parts = new bytes[](n);
            for (uint256 i; i < n; ++i) {
                parts[i] = abi.encode(i);
            }
            for (uint256 i; i < n; ++i) {
                total += parts[i].length;
            }
        }
    }

    function viaCallee(uint256 n) public pure returns (uint256 r) {
        /// @custom:solar-scratch
        {
            r = _build(n);
        }
    }

    function modulo(uint256 n) public pure returns (uint256 total) {
        /// @custom:solar-scratch
        {
            bytes[] memory parts = new bytes[](4);
            for (uint256 i; i < 4; ++i) {
                parts[i] = new bytes(0);
            }
            parts[n % 4] = abi.encode(n, n);
            for (uint256 i; i < 4; ++i) {
                total += parts[i].length * 2;
            }
        }
    }

    function _build(uint256 n) private pure returns (uint256) {
        uint256[][] memory rows = new uint256[][](n);
        for (uint256 i; i < n; ++i) {
            rows[i] = new uint256[](2);
        }
        return rows[n - 1].length + n;
    }
}
