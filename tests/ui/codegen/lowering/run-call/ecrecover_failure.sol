//@ codegen-matrix: standard
//@ run-call: recover => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: recoverInvalidDirtyScratch => 0x0000000000000000000000000000000000000000
//@ run-call: recoverUnderGasPressure => 0

// `ecrecover` returns `address(0)` only for a signature the precompile
// rejects, which returns no data, even when scratch space was written before.
// When the precompile call itself fails, as it does when too little gas is
// left for it, the recovery reverts with its return data, as solc's does,
// instead of returning `address(0)`.
contract EcrecoverFailure {
    address constant SIGNER = 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf;

    function recover() external pure returns (address) {
        return ecrecover(
            bytes32(uint256(1)),
            28,
            0x6673ffad2147741f04772b6f921f0ba6af0c1e77fc439e65c36dedf4092e8898,
            0x4c1a971652e0ada880120ef8025e709fff2080c4a39aae068d12eed009b68c89
        );
    }

    function recoverInvalidDirtyScratch() external pure returns (address) {
        assembly {
            mstore(0, not(0))
        }
        return ecrecover(bytes32(0), 0, bytes32(0), bytes32(0));
    }

    /// Counts the calls that returned anything but the signer, across gas limits around the
    /// precompile's cost.
    function recoverUnderGasPressure() external view returns (uint256 zeros) {
        for (uint256 g = 2000; g < 7000; g += 11) {
            try this.recover{gas: g}() returns (address a) {
                if (a != SIGNER) zeros++;
            } catch {}
        }
    }
}
