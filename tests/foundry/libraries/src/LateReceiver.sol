// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

struct Cell {
    uint256 x;
}

library LateLib {
    function digest(Cell memory cell, uint256[] memory values, uint256 y) external pure returns (bytes32) {
        return keccak256(abi.encode(cell, values, y));
    }
}

/// The receiver of an attached library call is read before the arguments run, as solc does, while
/// storage arguments are read after the last one.
contract LateReceiver {
    using LateLib for Cell;

    Cell internal cell;
    uint256[] internal values;

    function write() internal returns (uint256) {
        cell.x = 9;
        values.push(7);
        return 0;
    }

    function attached() external returns (bool) {
        bytes32 got = cell.digest(values, write());
        uint256[] memory after_ = new uint256[](1);
        after_[0] = 7;
        return got == keccak256(abi.encode(Cell(0), after_, 0));
    }

    function attachedTry() external returns (bool) {
        try cell.digest(values, write()) returns (bytes32 got) {
            uint256[] memory after_ = new uint256[](1);
            after_[0] = 7;
            return got == keccak256(abi.encode(Cell(0), after_, 0));
        } catch {
            return false;
        }
    }
}
