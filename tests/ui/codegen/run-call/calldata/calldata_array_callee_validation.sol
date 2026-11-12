//@ codegen-matrix: standard
//@ run-call: 0xed77481300000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001007700000000000000000000000000000000000000000000000000000000beef => 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: 0xf4211c4b00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001007700000000000000000000000000000000000000000000000000000000beef => 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: 0xeae7722d00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001007700000000000000000000000000000000000000000000000000000000beef => 0x
//@ run-call-fail: 0xcbe181d100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001007700000000000000000000000000000000000000000000000000000000beef => 0x

// An `address[] calldata` argument whose one element has dirty upper bits.
// As in solc, an element is validated where it is read, copied or encoded,
// also inside an internal function the array is passed to: a callee that
// only counts the elements or passes the array on succeeds, one that reads
// or copies the element reverts.
contract CalldataArrayCalleeValidation {
    event Seen(uint256 index);

    function onlyLength(address[] calldata a) external returns (uint256) {
        _count(a);
        return a.length;
    }

    function passThrough(address[] calldata a) external returns (uint256) {
        return _same(a).length;
    }

    function readFirst(address[] calldata a) external returns (uint256) {
        return _first(a);
    }

    function copyFirst(address[] calldata a) external returns (uint256) {
        return _copyFirst(a);
    }

    function _count(address[] calldata a) internal {
        for (uint256 i; i != a.length; ++i) emit Seen(i);
    }

    function _same(address[] calldata a) internal returns (address[] calldata) {
        emit Seen(0);
        return a;
    }

    function _first(address[] calldata a) internal returns (uint256) {
        emit Seen(0);
        return uint256(uint160(a[0]));
    }

    function _copyFirst(address[] calldata a) internal returns (uint256) {
        emit Seen(0);
        address[] memory copy = a;
        return uint256(uint160(copy[0]));
    }
}
