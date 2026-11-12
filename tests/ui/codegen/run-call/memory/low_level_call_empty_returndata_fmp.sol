//@ codegen-matrix: standard
//@ run-call: run() => 32

// solc leaves the free memory pointer where it was and returns 0; see CODEGEN-020 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    function noop() external {}

    function run() external returns (uint256) {
        bytes memory input = abi.encodeWithSelector(this.noop.selector);
        uint256 beforePtr;
        assembly { beforePtr := mload(0x40) }
        (bool ok, bytes memory data) = address(this).call(input);
        require(ok && data.length == 0);
        uint256 afterPtr;
        assembly { afterPtr := mload(0x40) }
        return afterPtr - beforePtr;
    }
}
