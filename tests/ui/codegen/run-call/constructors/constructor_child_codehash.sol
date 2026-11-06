//@ compile-flags: -Ogas
//@ run-call: ok => true

// The Seaport conduit-controller shape: a constructor deploys a child with
// CREATE2 and records the child's code hashes in immutables.
contract Child {
    address private immutable _controller;

    constructor() {
        _controller = msg.sender;
    }
}

contract ConstructorChildCodehash {
    bytes32 private immutable _creationHash;
    bytes32 private immutable _runtimeHash;
    address private immutable _child;

    constructor() {
        _creationHash = keccak256(type(Child).creationCode);
        Child zero = new Child{ salt: bytes32(0) }();
        _child = address(zero);
        _runtimeHash = address(zero).codehash;
    }

    function ok() external view returns (bool) {
        return _creationHash == keccak256(type(Child).creationCode)
            && _runtimeHash == _child.codehash && _child != address(0);
    }
}
