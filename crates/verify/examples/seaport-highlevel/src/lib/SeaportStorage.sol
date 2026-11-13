// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { OrderStatus } from "seaport-types/src/lib/ConsiderationStructs.sol";

import {
    _REENTRANCY_GUARD_SLOT
} from "seaport-types/src/lib/ConsiderationConstants.sol";

/**
 * @title SeaportStorage
 * @notice Every storage variable of the high-level Seaport, declared once and
 *         in the order the optimized Seaport 1.6 lays them out, so both
 *         versions read and write the same slots:
 *
 *         slot 0            _counters
 *         slot 1            _orderStatus
 *         slot 2            _contractNonces
 *         slot 0x929eee14   _reentrancyGuardSstore (_REENTRANCY_GUARD_SLOT)
 *
 *         Every contract that holds state inherits this one first. The guard
 *         slot is reached with a padding array instead of assembly. In
 *         transient storage the guard is a transient variable; transient
 *         slots are private to this contract, so its slot number (0 here,
 *         _REENTRANCY_GUARD_SLOT in the optimized code) is not observable.
 */
contract SeaportStorage {
    // Slot 0.
    mapping(address => uint256) internal _counters;

    // Slot 1.
    mapping(bytes32 => OrderStatus) internal _orderStatus;

    // Slot 2.
    mapping(address => uint256) internal _contractNonces;

    // Slots 3 to _REENTRANCY_GUARD_SLOT - 1, never read or written.
    uint256[_REENTRANCY_GUARD_SLOT - 3] private __guardSlotPadding;

    // Slot _REENTRANCY_GUARD_SLOT: the guard on EVMs without TSTORE.
    uint256 internal _reentrancyGuardSstore;

    // The guard on EVMs with TSTORE.
    uint256 internal transient _reentrancyGuardTstore;
}
