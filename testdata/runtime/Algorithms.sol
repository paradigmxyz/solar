// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;
import {LibString} from "src/utils/LibString.sol";
import {LibSort} from "src/utils/LibSort.sol";
import {Base64} from "src/utils/Base64.sol";
// Ordinary ABI calls isolate library work from upstream test assertions.
contract Algorithms {
    function decimal(uint256 x) external pure returns (string memory) {
        return LibString.toString(x);
    }

    function signedDecimal(int256 x) external pure returns (string memory) {
        return LibString.toString(x);
    }

    function insertion(uint256[] memory a) external pure returns (uint256[] memory) {
        LibSort.insertionSort(a);
        return a;
    }

    function sort(uint256[] memory a) external pure returns (uint256[] memory) {
        LibSort.sort(a);
        return a;
    }

    function decode(string memory x) external pure returns (bytes memory) {
        return Base64.decode(x);
    }

}
