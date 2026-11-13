// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {
    ConduitControllerInterface
} from "seaport-types/src/interfaces/ConduitControllerInterface.sol";

import {
    ConsiderationEventsAndErrors
} from "seaport-types/src/interfaces/ConsiderationEventsAndErrors.sol";

import {
    ReentrancyErrors
} from "seaport-types/src/interfaces/ReentrancyErrors.sol";

/**
 * @title ConsiderationBase
 * @author 0age
 * @notice ConsiderationBase contains all storage, constants, and constructor
 *         logic.
 */
import {
    BulkOrder_Typehash_Height_One,
    BulkOrder_Typehash_Height_Two,
    BulkOrder_Typehash_Height_Three,
    BulkOrder_Typehash_Height_Four,
    BulkOrder_Typehash_Height_Five,
    BulkOrder_Typehash_Height_Six,
    BulkOrder_Typehash_Height_Seven,
    BulkOrder_Typehash_Height_Eight,
    BulkOrder_Typehash_Height_Nine,
    BulkOrder_Typehash_Height_Ten,
    BulkOrder_Typehash_Height_Eleven,
    BulkOrder_Typehash_Height_Twelve,
    BulkOrder_Typehash_Height_Thirteen,
    BulkOrder_Typehash_Height_Fourteen,
    BulkOrder_Typehash_Height_Fifteen,
    BulkOrder_Typehash_Height_Sixteen,
    BulkOrder_Typehash_Height_Seventeen,
    BulkOrder_Typehash_Height_Eighteen,
    BulkOrder_Typehash_Height_Nineteen,
    BulkOrder_Typehash_Height_Twenty,
    BulkOrder_Typehash_Height_TwentyOne,
    BulkOrder_Typehash_Height_TwentyTwo,
    BulkOrder_Typehash_Height_TwentyThree,
    BulkOrder_Typehash_Height_TwentyFour
} from "seaport-types/src/lib/ConsiderationConstants.sol";

import { SeaportStorage } from "./SeaportStorage.sol";

contract ReferenceConsiderationBase is
    SeaportStorage,
    ConsiderationEventsAndErrors,
    ReentrancyErrors
{
    // Declare constants for name, version, and reentrancy sentinel values.
    string internal constant _NAME = "Seaport";
    string internal constant _VERSION = "1.6";
    uint256 internal constant _NOT_ENTERED = 1;
    uint256 internal constant _ENTERED = 2;

    // Precompute hashes, original chainId, and domain separator on deployment.
    bytes32 internal immutable _NAME_HASH;
    bytes32 internal immutable _VERSION_HASH;
    bytes32 internal immutable _EIP_712_DOMAIN_TYPEHASH;
    bytes32 internal immutable _OFFER_ITEM_TYPEHASH;
    bytes32 internal immutable _CONSIDERATION_ITEM_TYPEHASH;
    bytes32 internal immutable _ORDER_TYPEHASH;
    bytes32 internal immutable _BULK_ORDER_TYPEHASH;
    uint256 internal immutable _CHAIN_ID;
    bytes32 internal immutable _DOMAIN_SEPARATOR;

    // Allow for interaction with the conduit controller.
    ConduitControllerInterface internal immutable _CONDUIT_CONTROLLER;

    // Cache the conduit creation code hash used by the conduit controller.
    bytes32 internal immutable _CONDUIT_CREATION_CODE_HASH;

    // Map bulk order tree height to its respective EIP-712 typehash.

    /**
     * @dev Derive and set hashes, reference chainId, and associated domain
     *      separator during deployment.
     *
     * @param conduitController           A contract that deploys conduits, or
     *                                    proxies that may optionally be used to
     *                                    transfer approved ERC20+721+1155
     *                                    tokens.
     */
    constructor(address conduitController) {
        // Derive name and version hashes alongside required EIP-712 typehashes.
        (
            _NAME_HASH,
            _VERSION_HASH,
            _EIP_712_DOMAIN_TYPEHASH,
            _OFFER_ITEM_TYPEHASH,
            _CONSIDERATION_ITEM_TYPEHASH,
            _ORDER_TYPEHASH,
            _BULK_ORDER_TYPEHASH,
            _DOMAIN_SEPARATOR
        ) = _deriveTypehashes();

        // Store the current chainId and derive the current domain separator.
        _CHAIN_ID = block.chainid;

        // Set supplied conduit controller to an in-memory controller interface.
        ConduitControllerInterface tempController =
            ConduitControllerInterface(conduitController);

        // Assign the in-memory interface as an immutable.
        _CONDUIT_CONTROLLER = tempController;

        // Retrieve the conduit creation code hash from the supplied controller.
        (_CONDUIT_CREATION_CODE_HASH,) = (tempController.getConduitCodeHashes());
    }

    /**
     * @dev Internal view function to derive the initial EIP-712 domain
     *      separator.
     *
     * @param _eip712DomainTypeHash      The primary EIP-712 domain typehash.
     * @param _nameHash                  The hash of the name of the contract.
     * @param _versionHash               The hash of the version string of the
     *                                   contract.
     *
     * @return domainSeparator           The derived domain separator.
     */
    function _deriveInitialDomainSeparator(
        bytes32 _eip712DomainTypeHash,
        bytes32 _nameHash,
        bytes32 _versionHash
    ) internal view virtual returns (bytes32 domainSeparator) {
        return _deriveDomainSeparator(
            _eip712DomainTypeHash, _nameHash, _versionHash
        );
    }

    /**
     * @dev Internal view function to derive the EIP-712 domain separator.
     *
     * @return The derived domain separator.
     */
    function _deriveDomainSeparator(
        bytes32 _eip712DomainTypeHash,
        bytes32 _nameHash,
        bytes32 _versionHash
    ) internal view virtual returns (bytes32) {
        return keccak256(
            abi.encode(
                _eip712DomainTypeHash,
                _nameHash,
                _versionHash,
                block.chainid,
                address(this)
            )
        );
    }

    /**
     * @dev Internal pure function to derive required EIP-712 typehashes and
     *      other hashes during contract creation.
     *
     * @return nameHash                  The hash of the name of the contract.
     * @return versionHash               The hash of the version string of the
     *                                   contract.
     * @return eip712DomainTypehash      The primary EIP-712 domain typehash.
     * @return offerItemTypehash         The EIP-712 typehash for OfferItem
     *                                   types.
     * @return considerationItemTypehash The EIP-712 typehash for
     *                                   ConsiderationItem types.
     * @return orderTypehash             The EIP-712 typehash for Order types.
     * @return bulkOrderTypeHash
     * @return domainSeparator           The domain separator.
     */
    function _deriveTypehashes()
        internal
        view
        returns (
            bytes32 nameHash,
            bytes32 versionHash,
            bytes32 eip712DomainTypehash,
            bytes32 offerItemTypehash,
            bytes32 considerationItemTypehash,
            bytes32 orderTypehash,
            bytes32 bulkOrderTypeHash,
            bytes32 domainSeparator
        )
    {
        // Derive hash of the name of the contract.
        nameHash = keccak256(bytes(_NAME));

        // Derive hash of the version string of the contract.
        versionHash = keccak256(bytes(_VERSION));

        // Construct the OfferItem type string.
        bytes memory offerItemTypeString = abi.encodePacked(
            "OfferItem(",
            "uint8 itemType,",
            "address token,",
            "uint256 identifierOrCriteria,",
            "uint256 startAmount,",
            "uint256 endAmount",
            ")"
        );

        // Construct the ConsiderationItem type string.
        bytes memory considerationItemTypeString = abi.encodePacked(
            "ConsiderationItem(",
            "uint8 itemType,",
            "address token,",
            "uint256 identifierOrCriteria,",
            "uint256 startAmount,",
            "uint256 endAmount,",
            "address recipient",
            ")"
        );

        // Construct the OrderComponents type string, not including the above.
        bytes memory orderComponentsPartialTypeString = abi.encodePacked(
            "OrderComponents(",
            "address offerer,",
            "address zone,",
            "OfferItem[] offer,",
            "ConsiderationItem[] consideration,",
            "uint8 orderType,",
            "uint256 startTime,",
            "uint256 endTime,",
            "bytes32 zoneHash,",
            "uint256 salt,",
            "bytes32 conduitKey,",
            "uint256 counter",
            ")"
        );

        // Construct the primary EIP-712 domain type string.
        eip712DomainTypehash = keccak256(
            abi.encodePacked(
                "EIP712Domain(",
                "string name,",
                "string version,",
                "uint256 chainId,",
                "address verifyingContract",
                ")"
            )
        );

        // Derive the OfferItem type hash using the corresponding type string.
        offerItemTypehash = keccak256(offerItemTypeString);

        // Derive ConsiderationItem type hash using corresponding type string.
        considerationItemTypehash = keccak256(considerationItemTypeString);

        // Derive OrderItem type hash via combination of relevant type strings.
        orderTypehash = keccak256(
            abi.encodePacked(
                orderComponentsPartialTypeString,
                considerationItemTypeString,
                offerItemTypeString
            )
        );

        // Encode the type string for the BulkOrder struct.
        bytes memory bulkOrderPartialTypeString = abi.encodePacked(
            "BulkOrder(OrderComponents[2][2][2][2][2][2][2] tree)"
        );

        // Generate the keccak256 hash of the concatenated type strings for the
        // BulkOrder, considerationItem, offerItem, and orderComponents.
        bulkOrderTypeHash = keccak256(
            abi.encodePacked(
                bulkOrderPartialTypeString,
                considerationItemTypeString,
                offerItemTypeString,
                orderComponentsPartialTypeString
            )
        );

        // Derive the initial domain separator using the domain typehash, the
        // name hash, and the version hash.
        domainSeparator = _deriveInitialDomainSeparator(
            eip712DomainTypehash, nameHash, versionHash
        );
    }

    /**
     * @dev Returns the bulk order typehash for a tree height, as the optimized
     *      lookup does: height 0 gives height two's and heights above 24 give
     *      height 24's (callers reject both first).
     */
    function _lookupBulkOrderTypehash(uint256 treeHeight)
        internal
        pure
        returns (bytes32)
    {
        if (treeHeight == 1) {
            return bytes32(BulkOrder_Typehash_Height_One);
        }
        if (treeHeight < 3) {
            return bytes32(BulkOrder_Typehash_Height_Two);
        }
        if (treeHeight == 3) {
            return bytes32(BulkOrder_Typehash_Height_Three);
        }
        if (treeHeight == 4) {
            return bytes32(BulkOrder_Typehash_Height_Four);
        }
        if (treeHeight == 5) {
            return bytes32(BulkOrder_Typehash_Height_Five);
        }
        if (treeHeight == 6) {
            return bytes32(BulkOrder_Typehash_Height_Six);
        }
        if (treeHeight == 7) {
            return bytes32(BulkOrder_Typehash_Height_Seven);
        }
        if (treeHeight == 8) {
            return bytes32(BulkOrder_Typehash_Height_Eight);
        }
        if (treeHeight == 9) {
            return bytes32(BulkOrder_Typehash_Height_Nine);
        }
        if (treeHeight == 10) {
            return bytes32(BulkOrder_Typehash_Height_Ten);
        }
        if (treeHeight == 11) {
            return bytes32(BulkOrder_Typehash_Height_Eleven);
        }
        if (treeHeight == 12) {
            return bytes32(BulkOrder_Typehash_Height_Twelve);
        }
        if (treeHeight == 13) {
            return bytes32(BulkOrder_Typehash_Height_Thirteen);
        }
        if (treeHeight == 14) {
            return bytes32(BulkOrder_Typehash_Height_Fourteen);
        }
        if (treeHeight == 15) {
            return bytes32(BulkOrder_Typehash_Height_Fifteen);
        }
        if (treeHeight == 16) {
            return bytes32(BulkOrder_Typehash_Height_Sixteen);
        }
        if (treeHeight == 17) {
            return bytes32(BulkOrder_Typehash_Height_Seventeen);
        }
        if (treeHeight == 18) {
            return bytes32(BulkOrder_Typehash_Height_Eighteen);
        }
        if (treeHeight == 19) {
            return bytes32(BulkOrder_Typehash_Height_Nineteen);
        }
        if (treeHeight == 20) {
            return bytes32(BulkOrder_Typehash_Height_Twenty);
        }
        if (treeHeight == 21) {
            return bytes32(BulkOrder_Typehash_Height_TwentyOne);
        }
        if (treeHeight == 22) {
            return bytes32(BulkOrder_Typehash_Height_TwentyTwo);
        }
        if (treeHeight == 23) {
            return bytes32(BulkOrder_Typehash_Height_TwentyThree);
        }
        return bytes32(BulkOrder_Typehash_Height_TwentyFour);
    }
}
