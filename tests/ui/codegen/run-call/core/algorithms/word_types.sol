//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: sorts 0 => true
//@ run-call: sorts 1 => true
//@ run-call: sorts 2 => true
//@ run-call: sorts 15 => true
//@ run-call: sorts 16 => true
//@ run-call: sorts 17 => true
//@ run-call: sorts 33 => true
//@ run-call: sorts 64 => true
//@ run-call: groups 17 => true
//@ run-call: groups 64 => true
//@ run-call: largeAlias 33 => true
//@ run-call: duplicateTypes 64, false => true
//@ run-call: duplicateTypes 64, true => true
//@ run-call: signedGroups [-2, 0, -2, 1], [4, 7, 5, 8] => [0, 1, -2], [7, 8, 9]
//@ run-call: aliasGroups [2, 1, 2] => [1, 4]
//@ run-call: aliasGroups [3, 1, 3, 2, 1] => [2, 2, 6]
//@ run-call-fail: signedGroups [0, 0], [115792089237316195423570985008687907853269984665640564039457584007913129639935, 1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: signedGroups [0, 1], [1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

import {WordArrays} from "solar:core/WordArrays.sol";

contract Test {
    function sorts(uint256 n) public pure returns (bool) {
        uint256[] memory u = new uint256[](n);
        int256[] memory s = new int256[](n);
        address[] memory a = new address[](n);
        bytes32[] memory b = new bytes32[](n);
        for (uint256 i; i < n; ++i) {
            uint256 value = n - i;
            u[i] = value;
            s[i] = int256(value) - int256(n / 2);
            a[i] = address(uint160(value));
            b[i] = bytes32(value);
        }
        WordArrays.sort(u);
        WordArrays.sort(s);
        WordArrays.sort(a);
        WordArrays.sort(b);
        for (uint256 i; i < n; ++i) {
            require(u[i] == i + 1 && s[i] == int256(i + 1) - int256(n / 2));
            require(a[i] == address(uint160(i + 1)) && b[i] == bytes32(i + 1));
        }
        return true;
    }

    function duplicateTypes(uint256 n, bool duplicate) public pure returns (bool) {
        uint256 capacity = 1;
        while (capacity < n * 2) capacity *= 2;
        int256[] memory s = new int256[](n);
        address[] memory a = new address[](n);
        bytes32[] memory b = new bytes32[](n);
        for (uint256 i; i < n; ++i) {
            uint256 value = (n - i) * capacity;
            s[i] = -int256(value);
            a[i] = address(uint160(value));
            b[i] = bytes32(value);
        }
        if (duplicate) {
            s[n - 1] = s[0];
            a[n - 1] = a[0];
            b[n - 1] = b[0];
        }
        bytes32 before = keccak256(abi.encode(s, a, b));
        require(WordArrays.hasDuplicate(s) == duplicate);
        require(WordArrays.hasDuplicate(a) == duplicate);
        require(WordArrays.hasDuplicate(b) == duplicate);
        return before == keccak256(abi.encode(s, a, b));
    }

    function signedGroups(int256[] memory keys, uint256[] memory values)
        public
        pure
        returns (int256[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    // Paired movement must also work when both parameters are the same memory array.
    function aliasGroups(uint256[] memory a) public pure returns (uint256[] memory) {
        WordArrays.groupSum(a, a);
        return a;
    }

    // Exercise paired heap movement with duplicates and distinct payloads in
    // every overload. Signed group keys retain raw word ordering.
    function groups(uint256 n) public pure returns (bool) {
        uint256[] memory u = new uint256[](n);
        int256[] memory s = new int256[](n);
        address[] memory a = new address[](n);
        bytes32[] memory b = new bytes32[](n);
        uint256[] memory uv = new uint256[](n);
        uint256[] memory sv = new uint256[](n);
        uint256[] memory av = new uint256[](n);
        uint256[] memory bv = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            uint256 key = (i * 13 + 7) % 5;
            u[i] = key;
            s[i] = int256(key) - 2;
            a[i] = address(uint160(key));
            b[i] = bytes32(key);
            uv[i] = i;
            sv[i] = i;
            av[i] = i;
            bv[i] = i;
        }
        WordArrays.groupSum(u, uv);
        WordArrays.groupSum(s, sv);
        WordArrays.groupSum(a, av);
        WordArrays.groupSum(b, bv);
        require(u.length == 5 && s.length == 5 && a.length == 5 && b.length == 5);
        for (uint256 key; key < 5; ++key) {
            uint256 sum;
            for (uint256 i; i < n; ++i) {
                if ((i * 13 + 7) % 5 == key) sum += i;
            }
            uint256 si = (key + 3) % 5;
            require(u[key] == key && a[key] == address(uint160(key)) && b[key] == bytes32(key));
            require(s[si] == int256(key) - 2);
            require(uv[key] == sum && av[key] == sum && bv[key] == sum && sv[si] == sum);
        }
        return true;
    }

    function largeAlias(uint256 n) public pure returns (bool) {
        uint256[] memory a = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = n - i;
        }
        WordArrays.groupSum(a, a);
        for (uint256 i; i < n; ++i) {
            require(a[i] == i + 1);
        }
        return true;
    }
}
