//@ codegen-matrix: standard
//@ run-call: uint80Array() => 0

// Copying a memory array over a packed storage array clears dirty bits that assembly wrote above
// its elements, like solc.
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
