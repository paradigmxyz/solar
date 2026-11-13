// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { Test } from "forge-std/Test.sol";

/// Deploys every Seaport variant at the same address with the same conduit
/// controller, so immutables (domain separator, conduit code hash) agree and
/// the runtime codes can be compared directly.
abstract contract SeaportVariants is Test {
    string internal constant OPTIMIZED =
        "lib/seaport/optimized-out/Seaport.sol/Seaport.json";
    string internal constant REFERENCE =
        "lib/seaport/reference-out/ReferenceConsideration.sol/ReferenceConsideration.json";
    /// The rewrite; SEAPORT_B in the environment names another artifact to
    /// compare with the optimized build, such as REFERENCE.
    string internal constant HIGHLEVEL =
        "ReferenceConsideration.sol:ReferenceConsideration";
    string internal constant CONTROLLER =
        "lib/seaport/optimized-out/ConduitController.sol/ConduitController.json";

    address internal controller;

    function _deployController() internal {
        controller = deployCode(CONTROLLER);
    }

    /// Deploys the artifact from a fresh snapshot so every variant lands at
    /// the same address, and returns that address and the runtime code.
    function _deployAt(string memory artifact)
        internal
        returns (address at, bytes memory runtime)
    {
        uint256 snap = vm.snapshotState();
        at = deployCode(artifact, abi.encode(controller));
        runtime = at.code;
        vm.revertToState(snap);
    }
}
