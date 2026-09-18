//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: shorten [44, 441], 2 => [44, 441]
//@ run-call: shorten [392, 3, 117, 265, 854, 538], 5 => [392, 3, 117, 265, 854]
//@ run-call: shorten [754, 89, 830, 98, 6, 321], 2 => [754, 89]
//@ run-call: shorten [476, 142, 931, 737, 929, 643, 153, 372], 8 => [476, 142, 931, 737, 929, 643, 153, 372]
//@ run-call: shorten [284, 84, 447, 436], 4 => [284, 84, 447, 436]
//@ run-call: shorten [], 0 => []
//@ run-call-fail: shorten [1, 2, 3], 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: shorten [], 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: throughAlias [317, 756, 970, 486, 101, 315, 533, 796], 1 => 8, 1, 1, 317
//@ run-call: throughAlias [432, 112, 106, 597], 1 => 4, 1, 1, 432
//@ run-call: throughAlias [536, 196, 689, 688, 688, 485, 533], 5 => 7, 5, 5, 2797
//@ run-call: throughAlias [150, 727, 30, 24], 0 => 4, 0, 0, 0
//@ run-call: indexAfter [7, 8, 9], 2, 1 => 8
//@ run-call-fail: indexAfter [7, 8, 9], 2, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: attached [0xe5bf710f3bca8b868f98c20256568ba477e7425f, 0x442a074f11916c3f40ccfd8c7ba19a637cecb016, 0x8c4de91894026328f76e48f3e3eb3ae49d673514], 2 => [0xe5bf710f3bca8b868f98c20256568ba477e7425f, 0x442a074f11916c3f40ccfd8c7ba19a637cecb016]
//@ run-call-fail: attached [0xe5bf710f3bca8b868f98c20256568ba477e7425f, 0x442a074f11916c3f40ccfd8c7ba19a637cecb016, 0x8c4de91894026328f76e48f3e3eb3ae49d673514], 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: shortenBytes 0xdd17ef1b581977d94c6ab37820db5c72e4fb44e353b9f73aef9824eca5b87c258fbe9d13b1beb057, 33 => 0xdd17ef1b581977d94c6ab37820db5c72e4fb44e353b9f73aef9824eca5b87c258f
//@ run-call: shortenBytes 0xdd17ef1b581977d94c6ab37820db5c72e4fb44e353b9f73aef9824eca5b87c258fbe9d13b1beb057, 0 => 0x
//@ run-call-fail: shortenBytes 0xdd17ef1b581977d94c6ab37820db5c72e4fb44e353b9f73aef9824eca5b87c258fbe9d13b1beb057, 41 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: shortenString "hello world", 5 => "hello"

// Truncation has no portable Solidity spelling, so the shipped body is one
// memory-safe assembly store of the length word and the intrinsic is a length
// write the alias and value-numbering analyses model. Both must agree, and the
// aliasing cases are the point: every reference sees the new length at once.
import {Arrays} from "solar:core/v1/Arrays.sol";

contract Test {
    using Arrays for address[];

    function shorten(uint256[] memory a, uint256 n) public pure returns (uint256[] memory) {
        Arrays.truncate(a, n);
        return a;
    }

    // The array reached through an alias observes the new length, and so do
    // the lengths read before and after: nothing may keep the old one.
    function throughAlias(uint256[] memory a, uint256 n)
        public
        pure
        returns (uint256 before, uint256 later, uint256 viaAlias, uint256 sum)
    {
        uint256[] memory b = a;
        before = a.length;
        Arrays.truncate(b, n);
        later = a.length;
        viaAlias = b.length;
        for (uint256 i; i < a.length; ++i) sum += a[i];
    }

    // Indexing after truncation checks against the new length.
    function indexAfter(uint256[] memory a, uint256 n, uint256 i) public pure returns (uint256) {
        Arrays.truncate(a, n);
        return a[i];
    }

    function attached(address[] memory a, uint256 n) public pure returns (address[] memory) {
        a.truncate(n);
        return a;
    }

    function shortenBytes(bytes memory b, uint256 n) public pure returns (bytes memory) {
        Arrays.truncate(b, n);
        return b;
    }

    function shortenString(string memory s, uint256 n) public pure returns (string memory) {
        Arrays.truncate(s, n);
        return s;
    }
}
