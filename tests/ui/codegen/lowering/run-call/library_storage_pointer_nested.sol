//@ revisions: homestead byzantium osaka
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
//@[osaka] compile-flags: --evm-version osaka
//@ run-call: C::memberWrite => 7
//@ run-call: C::valueWrite => 9
//@ run-call: C::nestedPop => 1
//@ run-call: C::readBack => 12

// A nested storage pointer crossing a library boundary is still one slot word, so the pointer
// the library returns indexes, writes and resizes the caller's own storage. An external library
// cannot be deployed by `run-call`, so these are internal calls, which also exercises the
// pointer path before Byzantium, where an external call's return data is unreadable.
library L {
    struct S {
        uint256[] arr;
        mapping(uint256 => uint256[]) byKey;
    }

    // A pointer to a struct member.
    function memberArr(S storage s) internal view returns (uint256[] storage) {
        return s.arr;
    }

    // A pointer to a mapping value.
    function valueArr(S storage s, uint256 key) internal view returns (uint256[] storage) {
        return s.byKey[key];
    }

    // A pointer to an element of an array of arrays.
    function nested(uint256[][] storage aa, uint256 i) internal view returns (uint256[] storage) {
        return aa[i];
    }
}

contract C {
    L.S private s;
    uint256[][] private aa;

    // A returned pointer used as an lvalue writes the caller's storage.
    function memberWrite() external returns (uint256) {
        s.arr.push(1);
        s.arr.push(2);
        L.memberArr(s)[1] = 7;
        return s.arr[1];
    }

    function valueWrite() external returns (uint256) {
        L.valueArr(s, 3).push(9);
        return s.byKey[3][0];
    }

    // Resizing through a returned pointer resizes the caller's array.
    function nestedPop() external returns (uint256) {
        aa.push();
        aa[0].push(1);
        aa[0].push(2);
        L.nested(aa, 0).pop();
        return aa[0].length;
    }

    // Reading through a pointer sees writes made through another pointer to the same object.
    function readBack() external returns (uint256) {
        L.memberArr(s).push(5);
        L.valueArr(s, 1).push(7);
        return L.memberArr(s)[0] + L.valueArr(s, 1)[0];
    }
}
