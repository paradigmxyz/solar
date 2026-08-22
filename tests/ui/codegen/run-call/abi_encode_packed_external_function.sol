//@ run-call: AbiEncodePackedExternalFunction::matches() => true

contract AbiEncodePackedExternalFunction {
    function target() external {}

    function matches() external view returns (bool) {
        bytes32 pointerHash = keccak256(abi.encodePacked(this.target));
        bytes32 partsHash = keccak256(abi.encodePacked(address(this), this.target.selector));
        return pointerHash == partsHash;
    }
}
