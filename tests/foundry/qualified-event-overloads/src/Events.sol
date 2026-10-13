// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IBase {
    event E(uint256 indexed a, bytes b);
}

interface IDerived is IBase {
    event E(uint64 a, bytes b);
}

library L {
    function emitDerived(uint64 a, bytes memory b) internal {
        emit IDerived.E(a, b);
    }
}

contract C is IDerived {
    function fromLibrary(uint64 a, bytes calldata b) external {
        L.emitDerived(a, b);
    }

    function derived(uint64 a, bytes calldata b) external {
        emit IDerived.E(a, b);
    }

    function base(uint64 a, bytes calldata b) external {
        emit IBase.E(a, b);
    }

    function unqualified(uint256 a, bytes calldata b) external {
        emit E(a, b);
    }
}
