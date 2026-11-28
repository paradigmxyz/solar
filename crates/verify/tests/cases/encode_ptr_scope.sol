pragma solidity ^0.8.20;
// A call by name runs a function pointer only where a function-typed variable of that name is
// in scope; `abi.encode` hash proofs stay exact otherwise and follow the pointer when it is.
contract NestedScalar {
    function _peek(uint256[1] memory) private pure returns (uint256) {
        return 2;
    }

    function _write(uint256[1] memory a) private pure returns (uint256) {
        a[0] = 1;
        return 2;
    }

    // A scalar named `_peek` in a closed block cannot be called and is out of scope at the call.
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function ok_nested_scalar(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        {
            uint256 _peek = x;
            require(_peek == x);
        }
        r = keccak256(abi.encode(a, _peek(a)));
    }
}

contract Root {
    uint256 public digest;

    constructor(uint256 d) {
        digest = d;
    }
}

// The base initializer runs in this constructor's scope, where `_touch` is the pointer
// parameter, not the no-op function.
abstract contract Middle is Root {
    function _touch(uint256[1] memory) internal pure returns (uint256) {
        return 2;
    }

    function _write(uint256[1] memory a) internal pure returns (uint256) {
        a[0] = 1;
        return 2;
    }

    constructor(
        uint256[1] memory a,
        function(uint256[1] memory) internal pure returns (uint256) _touch
    ) Root(uint256(keccak256(abi.encode(a, _touch(a))))) {}
}

contract Leaf is Middle {
    function _fresh() private pure returns (uint256[1] memory a) {
        a[0] = 42;
    }

    // expect: solver returned unknown
    /// @custom:ensures digest == uint256(keccak256(abi.encode(uint256(42), uint256(2))))
    constructor() Middle(_fresh(), _write) {}
}
