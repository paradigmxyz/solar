// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { Differential } from "./Differential.sol";

/// Order hashing and cancellation, from orders built with raw uint8 enum
/// fields (so out-of-range item and order types reach the contracts) and
/// from byte-level mutations of their calldata.
contract OrderHashTest is Differential {
    struct RawOfferItem {
        uint8 itemType;
        address token;
        uint256 identifierOrCriteria;
        uint256 startAmount;
        uint256 endAmount;
    }

    struct RawConsiderationItem {
        uint8 itemType;
        address token;
        uint256 identifierOrCriteria;
        uint256 startAmount;
        uint256 endAmount;
        address recipient;
    }

    /// OrderComponents with uint8 enums: same ABI encoding as Seaport's.
    struct RawOrderComponents {
        address offerer;
        address zone;
        RawOfferItem[] offer;
        RawConsiderationItem[] consideration;
        uint8 orderType;
        uint256 startTime;
        uint256 endTime;
        bytes32 zoneHash;
        uint256 salt;
        bytes32 conduitKey;
        uint256 counter;
    }

    string internal constant COMPONENTS =
        "(address,address,(uint8,address,uint256,uint256,uint256)[],(uint8,address,uint256,uint256,uint256,address)[],uint8,uint256,uint256,bytes32,uint256,bytes32,uint256)";

    bytes4 internal constant GET_ORDER_HASH = 0x79df72bd;
    bytes4 internal constant CANCEL = 0xfd9f1e10;

    function setUp() public {
        _setUpVariants(OPTIMIZED, vm.envOr("SEAPORT_B", HIGHLEVEL));
    }

    function _order(uint256 seed, address offerer, uint8 orderType, uint8 nOffer, uint8 nCons)
        internal
        pure
        returns (RawOrderComponents memory o)
    {
        o.offerer = offerer;
        o.zone = address(uint160(uint256(keccak256(abi.encode(seed, "zone")))));
        o.orderType = orderType;
        o.startTime = seed % 1000;
        o.endTime = uint256(keccak256(abi.encode(seed, "end")));
        o.zoneHash = keccak256(abi.encode(seed, "zh"));
        o.salt = seed;
        o.conduitKey = keccak256(abi.encode(seed, "ck"));
        o.counter = seed >> 128;
        o.offer = new RawOfferItem[](nOffer % 4);
        for (uint256 i; i < o.offer.length; ++i) {
            uint256 r = uint256(keccak256(abi.encode(seed, "o", i)));
            o.offer[i] = RawOfferItem(uint8(r), address(uint160(r >> 8)), r >> 3, r >> 5, r >> 7);
        }
        o.consideration = new RawConsiderationItem[](nCons % 4);
        for (uint256 i; i < o.consideration.length; ++i) {
            uint256 r = uint256(keccak256(abi.encode(seed, "c", i)));
            o.consideration[i] =
                RawConsiderationItem(uint8(r % 7), address(uint160(r >> 8)), r >> 3, r >> 5, r >> 7, address(uint160(r >> 2)));
        }
    }

    /// A byte-level mutation of the calldata.
    struct Mutation {
        uint256 where;
        bytes32 word;
        uint256 cut;
        uint8 how;
    }

    /// Overwrites the word at a fuzzed offset (how bit 0) and cuts the
    /// calldata at a fuzzed length (how bit 1).
    function _mutate(bytes memory data, Mutation memory m)
        internal
        pure
        returns (bytes memory)
    {
        uint256 where = m.where;
        bytes32 word = m.word;
        uint256 cut = m.cut;
        uint8 how = m.how;
        if (how & 1 != 0 && data.length >= 36) {
            uint256 at = 4 + (where % (data.length - 35));
            for (uint256 i; i < 32; ++i) {
                data[at + i] = word[i];
            }
        }
        if (how & 2 != 0) {
            uint256 len = cut % (data.length + 1);
            assembly ("memory-safe") {
                mstore(data, len)
            }
        }
        return data;
    }

    function testFuzz_getOrderHash(
        uint256 seed,
        uint8 orderType,
        uint8 nOffer,
        uint8 nCons,
        Mutation memory m
    ) public {
        RawOrderComponents memory o = _order(seed, address(uint160(seed)), orderType, nOffer, nCons);
        bytes memory data = abi.encodePacked(GET_ORDER_HASH, abi.encode(o));
        _diff(address(this), 0, _mutate(data, m));
    }

    function testFuzz_cancel(
        uint256 seed,
        uint8 orderType,
        uint8 nOrders,
        uint8 callerIs,
        Mutation memory m
    ) public {
        if (m.how % 4 == 3) m.how = 0;
        address offerer = address(uint160(uint256(keccak256(abi.encode(seed, "offerer")))));
        RawOrderComponents[] memory orders = new RawOrderComponents[](nOrders % 3);
        for (uint256 i; i < orders.length; ++i) {
            orders[i] = _order(seed ^ i, offerer, i == 0 ? orderType % 6 : uint8(i), uint8(seed >> 8), uint8(seed >> 16));
        }
        address caller = callerIs % 3 == 0
            ? offerer
            : callerIs % 3 == 1 && orders.length > 0 ? orders[0].zone : address(0xBEEF);
        bytes memory data = abi.encodePacked(CANCEL, abi.encode(orders));
        _diff(caller, 0, _mutate(data, m));
    }

    /// D2: an offer item of type 6 is hashed as its raw byte; the
    /// reference's ABI decoder reverted with no data.
    function test_D2_getOrderHashOutOfRangeItemType() public {
        RawOrderComponents memory o = _order(1, address(0xA11CE), 0, 1, 0);
        o.offer[0].itemType = 6;
        Outcome memory r = _diff(address(this), 0, abi.encodePacked(GET_ORDER_HASH, abi.encode(o)));
        assertTrue(r.ok);
    }

    /// D3: cancel checks every order before reverting with
    /// CannotCancelOrder, so a later order whose order type is out of range
    /// reverts with no data first; the reference reverted at the first
    /// order the caller could not cancel.
    function test_D3_cancelRevertsAfterLoop() public {
        RawOrderComponents[] memory orders = new RawOrderComponents[](2);
        orders[0] = _order(1, address(0xA11CE), 0, 0, 0);
        orders[1] = _order(2, address(0xA11CE), 5, 0, 0);
        Outcome memory r = _diff(address(0xBEEF), 0, abi.encodePacked(CANCEL, abi.encode(orders)));
        assertFalse(r.ok);
        assertEq(r.ret.length, 0);
    }
}
