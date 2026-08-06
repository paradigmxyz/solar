// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LiteralDataDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        require(
            value < 1 << 248,
            "Compiler literal data can contain C, 9, and other opcode bytes"
        );
        return value + 1;
    }

    function probeSigned(int256 value) external pure returns (int256) {
        require(
            value < 1 << 247,
            "Compiler literal data can contain C, 9, and other opcode bytes: 248"
        );
        return value - 1;
    }

    function probeNarrowA(uint256 value) external pure returns (uint248) {
        require(
            value < 1 << 248,
            "Compiler literal data can contain C, 9, and other opcode bytes: 240"
        );
        return uint248(value);
    }

    function probeNarrowB(uint256 value) external pure returns (uint240) {
        require(
            value < 1 << 240,
            "Compiler literal data can contain C, 9, and other opcode bytes: 232"
        );
        return uint240(value);
    }

    function probeNarrowC(uint256 value) external pure returns (uint232) {
        require(
            value < 1 << 232,
            "Compiler literal data can contain C, 9, and other opcode bytes: 224"
        );
        return uint232(value);
    }
}
