// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Deploying supplied initcode.
/// @dev Compiler-owned module, imported as `solar:core/v1/Create.sol`.
/// `new C()` deploys a contract the compiler knows; these deploy whatever
/// bytes they are given, which clone factories and code-storage libraries
/// build at runtime. A failed creation reverts with `DeploymentFailed()`.
/// The bodies are one memory-safe assembly creation each, and the compiler
/// lowers the calls to the creation instruction with the same operands.
library Create {
    /// @dev The creation returned no address.
    error DeploymentFailed();

    /// @dev Deploys `initcode` with `value` wei and returns the new address.
    function deploy(bytes memory initcode, uint256 value) internal returns (address deployed) {
        assembly ("memory-safe") {
            deployed := create(value, add(initcode, 0x20), mload(initcode))
        }
        if (deployed == address(0)) revert DeploymentFailed();
    }

    /// @dev Deploys `initcode` with `value` wei at the address `salt` selects.
    function deploy2(bytes memory initcode, bytes32 salt, uint256 value)
        internal
        returns (address deployed)
    {
        assembly ("memory-safe") {
            deployed := create2(value, add(initcode, 0x20), mload(initcode), salt)
        }
        if (deployed == address(0)) revert DeploymentFailed();
    }

    /// @dev Like `deploy`, but a creation that returns no address is `false`
    /// and the zero address, not a revert.
    function tryDeploy(bytes memory initcode, uint256 value)
        internal
        returns (bool success, address deployed)
    {
        assembly ("memory-safe") {
            deployed := create(value, add(initcode, 0x20), mload(initcode))
        }
        success = deployed != address(0);
    }

    /// @dev Like `deploy2`, but a creation that returns no address is `false`
    /// and the zero address, not a revert.
    function tryDeploy2(bytes memory initcode, bytes32 salt, uint256 value)
        internal
        returns (bool success, address deployed)
    {
        assembly ("memory-safe") {
            deployed := create2(value, add(initcode, 0x20), mload(initcode), salt)
        }
        success = deployed != address(0);
    }

    /// @dev The address `deploy2` gives `deployer` for `salt` and initcode
    /// hashing to `initcodeHash`. Computes only; deploys nothing.
    function predict2(address deployer, bytes32 salt, bytes32 initcodeHash)
        internal
        pure
        returns (address)
    {
        return address(
            uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), deployer, salt, initcodeHash))))
        );
    }
}
