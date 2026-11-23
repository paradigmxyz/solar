pragma solidity ^0.8.20;
// Names that look like storage or calldata but, in the body that runs, name a memory object: a
// callee's write through its memory parameter must reach them.
contract Shadow {
    uint256[3] internal xs;

    function _fill(uint256[3] memory a) private pure {
        a[0] = 7;
    }

    // A callee's memory local named like the caller's calldata parameter.
    function _inner() private pure returns (uint256) {
        uint256[3] memory data;
        _fill(data);
        return data[0];
    }

    /// @custom:ensures result == 0
    // expect: postcondition not satisfied
    function bad_calldata_name(uint256[3] calldata data) external pure returns (uint256 result) {
        result = _inner() + data[0] * 0;
    }

    // A local that shadows the state variable `xs`.
    /// @custom:ensures result == 0
    // expect: postcondition not satisfied
    function bad_state_name() external pure returns (uint256 result) {
        uint256[3] memory xs;
        _fill(xs);
        result = xs[0];
    }

    // The return name declared again in an inner block: the function returns the caller's
    // object, not a fresh one.
    function _leak(uint256[3] memory keep) private pure returns (uint256[3] memory r) {
        {
            uint256[3] memory r = keep;
            return r;
        }
    }

    /// @custom:ensures result == 0
    // expect: may alias a caller object the callee writes
    function bad_shadowed_return(uint256[3] memory keep) external pure returns (uint256 result) {
        _fill(_leak(keep));
        result = keep[0];
    }

    function _mk(uint256[3] memory) private pure returns (uint256[3] memory r) {
        r[0] = 1;
    }

    function _id(uint256[3] memory a) private pure returns (uint256[3] memory) {
        return a;
    }

    // A function pointer local that shadows the fresh-returning `_mk`: the call returns the
    // caller's object.
    /// @custom:ensures result == 0
    // expect: may alias a caller object the callee writes
    function bad_fnptr_shadow(uint256[3] memory keep) external pure returns (uint256 result) {
        function(uint256[3] memory) pure returns (uint256[3] memory) _mk = _id;
        _fill(_mk(keep));
        result = keep[0];
    }

    // A memory local in an inner block that shadows a calldata parameter.
    /// @custom:ensures result == 0
    // expect: postcondition not satisfied
    function bad_inner_calldata_shadow(uint256[3] calldata data) external pure returns (uint256 result) {
        {
            uint256[3] memory data;
            _fill(data);
            result = data[0];
        }
    }
}
