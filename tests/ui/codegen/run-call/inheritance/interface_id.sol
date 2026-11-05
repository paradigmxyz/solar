//@ run-call: parentId => 0xc6be8b58
//@ run-call: derivedId => 0x85295877
//@ run-call: emptyId => 0x00000000
//@ run-call: abstractId => 0x85295877

interface Empty {}

interface Parent {
    function hello() external pure;
    function world(int256) external pure;
}

interface Derived is Parent {
    function other() external pure;
}

abstract contract Abstract is Parent {
    function other() external virtual;
}

contract InterfaceIds {
    function abstractId() external pure returns (bytes4) {
        return type(Abstract).interfaceId;
    }

    function parentId() external pure returns (bytes4) {
        return type(Parent).interfaceId;
    }

    function derivedId() external pure returns (bytes4) {
        return type(Derived).interfaceId;
    }

    function emptyId() external pure returns (bytes4) {
        return type(Empty).interfaceId;
    }
}
