//@ codegen-matrix: standard
//@ run-call: failsCheaply => true

contract DecodeBytesBadLength {
    function badLength() external pure returns (bytes memory) {
        // The payload starts at the second word and claims 2^60 bytes, past the end of the input.
        bytes memory encoded = abi.encode(uint256(32), uint256(1) << 60);
        return abi.decode(encoded, (bytes));
    }

    // The decode must revert on the length before it touches memory near the claimed end, which
    // would burn all the gas of the call instead.
    function failsCheaply() external view returns (bool) {
        uint256 before = gasleft();
        try this.badLength() {
            return false;
        } catch {
            return before - gasleft() < 100_000;
        }
    }
}
