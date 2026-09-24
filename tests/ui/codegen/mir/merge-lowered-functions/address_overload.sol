//@ compile-flags: -Osize -Zdump=mir
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:
//@ run-call: sortedWords [1, 2, 2] => true
//@ run-call: sortedWords [2, 1] => false
//@ run-call: sortedAddresses [0x0000000000000000000000000000000000000001, 0xffffffffffffffffffffffffffffffffffffffff] => true
//@ run-call: sortedAddresses [0xffffffffffffffffffffffffffffffffffffffff, 0x0000000000000000000000000000000000000001] => false

// The address overload proves its elements canonical, so its loads need no
// masks and its body lowers to the word overload's, which both wrappers then
// call.
contract C {
    // CHECK-LABEL: fn @sortedWords{{[( ]}}
    // CHECK: icall @[[SORTED:isSorted[.0-9]*]],
    function sortedWords(uint256[] memory a) public pure returns (bool) {
        return isSorted(a);
    }

    // CHECK-LABEL: fn @sortedAddresses{{[( ]}}
    // CHECK: icall @[[SORTED]],
    function sortedAddresses(address[] memory a) public pure returns (bool) {
        return isSorted(a);
    }

    function isSorted(uint256[] memory a) internal pure returns (bool) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }

    function isSorted(address[] memory a) internal pure returns (bool) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }
}
