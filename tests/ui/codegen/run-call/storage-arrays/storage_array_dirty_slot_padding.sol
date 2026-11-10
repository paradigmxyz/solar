//@ codegen-matrix: standard
//@ run-call: uint80Array() => 65535

// Dirty bits that assembly wrote above the elements of a packed storage array survive a copy over
// it; solc clears them and returns 0. See CODEGEN-018 in docs/SOLC_DIVERGENCE.md.
contract C {
    uint80[] words80;

    function uint80Array() external returns (uint256 r) {
        words80.push(1);
        assembly {
            mstore(0, words80.slot)
            sstore(keccak256(0, 32), not(0))
        }
        uint80[] memory m = new uint80[](3);
        words80 = m;
        assembly {
            mstore(0, words80.slot)
            r := shr(240, sload(keccak256(0, 32)))
        }
    }
}
