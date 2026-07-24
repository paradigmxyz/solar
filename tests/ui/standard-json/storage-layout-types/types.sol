//@ check-pass
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.36;

contract LayoutTypes {
    enum Choice {
        A,
        B
    }

    type Small is uint24;

    struct Packed {
        uint8 head;
        bytes17[3] values;
        uint8 tail;
    }

    bytes raw;
    string text;
    uint8[][2] nestedDynamic;
    bytes17[3] unevenArray;
    uint24[21] oddWidthArray;
    Packed packed;
    mapping(address => Packed) records;
    mapping(address => uint8[]) dynamicRecords;
    Choice choice;
    Small small;
    function(uint8[][2] calldata, bytes[] memory) external view returns (string[] memory) externalFn;
    function(uint256) internal pure returns (bool) internalFn;
    address payable recipient;
}
