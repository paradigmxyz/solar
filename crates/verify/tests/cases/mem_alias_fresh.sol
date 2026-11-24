pragma solidity ^0.8.20;
// Memory arguments the caller has no other name for: whatever the callee writes through its
// parameter, no name of the caller sees it.
contract Fresh {
    function _len(bytes memory data) private pure returns (uint256) {
        bytes memory d = data;
        return d.length > 0 ? 7 : 7;
    }

    function _fill(uint256[3] memory xs) private pure returns (uint256) {
        xs[0] = 1;
        return xs[0];
    }

    function _make() private pure returns (uint256[3] memory r) {
        r[1] = 2;
    }

    function _leak(uint256[3] memory keep) private pure returns (uint256[3] memory) {
        return keep;
    }

    /// @custom:ensures result == 7
    function ok_encode(uint256 a, uint256 b) external pure returns (uint256 result) {
        result = _len(abi.encode(a, b));
    }

    /// @custom:ensures result == 1
    function ok_fresh_call() external pure returns (uint256 result) {
        result = _fill(_make());
    }

    // expect: may alias a caller object the callee writes
    function bad_leak(uint256[3] memory keep) external pure returns (uint256) {
        return _fill(_leak(keep));
    }
}
