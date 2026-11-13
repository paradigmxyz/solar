// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {
    ConsiderationEventsAndErrors
} from "seaport-types/src/interfaces/ConsiderationEventsAndErrors.sol";

import { ReferenceReentrancyGuard } from "./ReferenceReentrancyGuard.sol";

/**
 * @title CounterManager
 * @author 0age
 * @notice CounterManager contains a storage mapping and related functionality
 *         for retrieving and incrementing a per-offerer counter.
 */
contract ReferenceCounterManager is
    ConsiderationEventsAndErrors,
    ReferenceReentrancyGuard
{

    /**
     * @dev Internal function to cancel all orders from a given offerer in bulk
     *      by incrementing a counter. Note that only the offerer may increment
     *      the counter. Note that the counter is incremented by a large,
     *      quasi-random interval, which makes it infeasible to "activate"
     *      signed orders by incrementing the counter.  This activation
     *      functionality can be achieved instead with restricted orders or
     *      contract orders.
     *
     * @return newCounter The new counter.
     */
    function _incrementCounter() internal returns (uint256 newCounter) {
        // The optimized code rejects calls made while the guard is held.
        _assertNonReentrant();

        // Use second half of the previous block hash as a quasi-random number.
        uint256 quasiRandomNumber = uint256(blockhash(block.number - 1)) >> 128;

        // Retrieve the original counter value.
        uint256 originalCounter = _counters[msg.sender];

        // Increment current counter for the supplied offerer.
        // The optimized code adds without an overflow check, so a counter
        // near 2^256 wraps instead of reverting.
        unchecked {
            newCounter = quasiRandomNumber + originalCounter;
        }

        // Update the counter with the new value.
        _counters[msg.sender] = newCounter;

        // Emit an event containing the new counter.
        emit CounterIncremented(newCounter, msg.sender);
    }

    /**
     * @dev Internal view function to retrieve the current counter for a given
     *      offerer.
     *
     * @param offerer The offerer in question.
     *
     * @return currentCounter The current counter.
     */
    function _getCounter(address offerer)
        internal
        view
        returns (uint256 currentCounter)
    {
        // Return the counter for the supplied offerer.
        currentCounter = _counters[offerer];
    }
}
