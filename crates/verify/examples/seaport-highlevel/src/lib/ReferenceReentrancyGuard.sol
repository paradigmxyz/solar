// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {
    ConsiderationEventsAndErrors
} from "seaport-types/src/interfaces/ConsiderationEventsAndErrors.sol";

import {
    ReentrancyErrors
} from "seaport-types/src/interfaces/ReentrancyErrors.sol";

import {
    _ENTERED_AND_ACCEPTING_NATIVE_TOKENS_SSTORE,
    _ENTERED_AND_ACCEPTING_NATIVE_TOKENS_TSTORE,
    _ENTERED_SSTORE,
    _ENTERED_TSTORE,
    _NOT_ENTERED_SSTORE,
    _NOT_ENTERED_TSTORE,
    _TSTORE_ENABLED_SSTORE
} from "seaport-types/src/lib/ConsiderationConstants.sol";

import { SeaportStorage } from "./SeaportStorage.sol";

/**
 * @notice A contract whose fallback reads transient storage, so calling it
 *         fails on an EVM without TLOAD. The optimized code deploys the same
 *         kind of probe from raw init code; this one is a Solidity contract,
 *         so its code (not its address or how Seaport calls it) differs.
 */
contract TloadProbe {
    uint256 private transient _probe;

    fallback() external {
        _probe;
    }
}

/**
 * @title ReferenceReentrancyGuard
 * @notice The optimized Seaport 1.6 guard in high-level Solidity: TSTORE when
 *         the EVM supported it at deployment, otherwise SSTORE at
 *         _REENTRANCY_GUARD_SLOT until __activateTstore switches to TSTORE.
 *         Every check, write and revert happens in the same order as in
 *         seaport-core's ReentrancyGuard.
 */
contract ReferenceReentrancyGuard is
    SeaportStorage,
    ConsiderationEventsAndErrors,
    ReentrancyErrors
{
    bool private immutable _tstoreInitialSupport;

    address private immutable _tloadTestContract;

    constructor() {
        address tloadTestContract = address(new TloadProbe());

        if (tloadTestContract == address(0)) {
            revert TloadTestContractDeploymentFailed();
        }

        bool tstoreInitialSupport = _testTload(tloadTestContract);

        _tstoreInitialSupport = tstoreInitialSupport;
        _tloadTestContract = tloadTestContract;

        if (!tstoreInitialSupport) {
            _reentrancyGuardSstore = _NOT_ENTERED_SSTORE;
        }
    }

    /**
     * @dev Switches the guard to TSTORE once the EVM supports it.
     */
    function __activateTstore() external {
        bool tstoreActivatable = _reentrancyGuardSstore == _NOT_ENTERED_SSTORE;

        if (_tstoreInitialSupport || !tstoreActivatable) {
            revert TStoreAlreadyActivated();
        }

        if (!_testTload(_tloadTestContract)) {
            revert TStoreNotSupported();
        }

        _reentrancyGuardSstore = _TSTORE_ENABLED_SSTORE;
    }

    modifier notEntered() {
        _assertNonReentrant();
        _;
    }

    modifier nonReentrant(bool acceptNativeTokens) {
        _setReentrancyGuard(acceptNativeTokens);
        _;
        _clearReentrancyGuard();
    }

    function _setReentrancyGuard(bool acceptNativeTokens) internal {
        uint256 entered = acceptNativeTokens ? 1 : 0;

        if (_tstoreInitialSupport) {
            if (_reentrancyGuardTstore != _NOT_ENTERED_TSTORE) {
                revert NoReentrantCalls();
            }
            _reentrancyGuardTstore = _ENTERED_TSTORE + entered;
            return;
        }

        uint256 reentrancyGuard = _reentrancyGuardSstore;

        if (reentrancyGuard == _TSTORE_ENABLED_SSTORE) {
            if (_reentrancyGuardTstore != _NOT_ENTERED_TSTORE) {
                revert NoReentrantCalls();
            }
            _reentrancyGuardTstore = _ENTERED_TSTORE + entered;
            return;
        }

        if (reentrancyGuard != _NOT_ENTERED_SSTORE) {
            revert NoReentrantCalls();
        }

        _reentrancyGuardSstore = _ENTERED_SSTORE + entered;
    }

    function _clearReentrancyGuard() internal {
        if (_tstoreInitialSupport) {
            _reentrancyGuardTstore = _NOT_ENTERED_TSTORE;
            return;
        }

        if (_reentrancyGuardSstore == _TSTORE_ENABLED_SSTORE) {
            _reentrancyGuardTstore = _NOT_ENTERED_TSTORE;
            return;
        }

        _reentrancyGuardSstore = _NOT_ENTERED_SSTORE;
    }

    function _assertNonReentrant() internal view {
        if (_tstoreInitialSupport) {
            if (_reentrancyGuardTstore != _NOT_ENTERED_TSTORE) {
                revert NoReentrantCalls();
            }
            return;
        }

        uint256 reentrancyGuard = _reentrancyGuardSstore;

        if (reentrancyGuard == _TSTORE_ENABLED_SSTORE) {
            if (_reentrancyGuardTstore != _NOT_ENTERED_TSTORE) {
                revert NoReentrantCalls();
            }
        }

        if (reentrancyGuard > _NOT_ENTERED_SSTORE) {
            revert NoReentrantCalls();
        }
    }

    function _assertAcceptingNativeTokens() internal view {
        if (_tstoreInitialSupport) {
            if (
                _reentrancyGuardTstore
                    != _ENTERED_AND_ACCEPTING_NATIVE_TOKENS_TSTORE
            ) {
                revert InvalidMsgValue(msg.value);
            }
            return;
        }

        uint256 reentrancyGuard = _reentrancyGuardSstore;

        if (reentrancyGuard == _TSTORE_ENABLED_SSTORE) {
            if (
                _reentrancyGuardTstore
                    != _ENTERED_AND_ACCEPTING_NATIVE_TOKENS_TSTORE
            ) {
                revert InvalidMsgValue(msg.value);
            }
        }

        if (
            reentrancyGuard != _TSTORE_ENABLED_SSTORE
                && reentrancyGuard != _ENTERED_AND_ACCEPTING_NATIVE_TOKENS_SSTORE
        ) {
            revert InvalidMsgValue(msg.value);
        }
    }

    function _testTload(address tloadTestContract)
        private
        view
        returns (bool ok)
    {
        (ok,) = tloadTestContract.staticcall{ gas: gasleft() / 10 }("");
    }
}
