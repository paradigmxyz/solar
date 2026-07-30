//@ run-call: parentId() => 0xc6be8b58
//@ run-call: derivedId() => 0x85295877
//@ run-call: emptyId() => 0x00000000

interface Empty {}

interface Parent {
    function hello() external pure;
    function world(int256) external pure;
}

interface Derived is Parent {
    function other() external pure;
}

contract InterfaceIds {
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
