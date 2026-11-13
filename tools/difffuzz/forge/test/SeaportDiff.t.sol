// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ConsiderationInterface} from "seaport-types/src/interfaces/ConsiderationInterface.sol";
import {
    AdditionalRecipient,
    BasicOrderParameters,
    ConsiderationItem,
    OfferItem,
    OrderComponents
} from "seaport-types/src/lib/ConsiderationStructs.sol";
import {BasicOrderType, ItemType, OrderType} from "seaport-types/src/lib/ConsiderationEnums.sol";
import {TestERC721} from "seaport/test/TestERC721.sol";

/// Four builds of Seaport: the optimized (assembly-heavy) and the reference (high-level)
/// implementation, each compiled by solc and by the compiler under test, with their own tokens
/// so balances compare one to one.
///
/// Same-source builds must agree on everything, revert data included. The optimized and the
/// reference implementation must agree on success and on return data, but may revert with
/// different data, as Seaport's own tests allow.
abstract contract Builds is Test {
    uint256 constant N = 4;

    ConsiderationInterface[N] internal seaport;
    TestERC721[N] internal nft;
    string[N] internal names = ["solc-optimized", "solc-reference", "solar-optimized", "solar-reference"];

    struct Basic {
        uint256 key;
        uint256 tokenId;
        uint128 price;
        uint128 tip;
        uint256 salt;
        uint32 start;
        uint32 duration;
        uint8 extra;
        bool cancel;
    }

    function _deploy() internal {
        string[N] memory paths = [
            "lib/seaport/optimized-out/Consideration.sol/Consideration.json",
            "lib/seaport/reference-out/ReferenceConsideration.sol/ReferenceConsideration.json",
            "lib/seaport/solar-optimized-out/Consideration.sol/Consideration.json",
            "lib/seaport/solar-reference-out/ReferenceConsideration.sol/ReferenceConsideration.json"
        ];
        address controller = deployCode("lib/seaport/optimized-out/ConduitController.sol/ConduitController.json");
        for (uint256 i; i < N; i++) {
            seaport[i] = ConsiderationInterface(deployCode(paths[i], abi.encode(controller)));
            nft[i] = new TestERC721();
            vm.label(address(seaport[i]), names[i]);
        }
    }

    /// Calls each build with the calldata made for it; returns a description of the first
    /// difference, or an empty string.
    function _run(bytes[N] memory data, uint256 value) internal returns (bool[N] memory ok, string memory diff) {
        bytes[N] memory ret;
        for (uint256 i; i < N; i++) {
            (ok[i], ret[i]) = address(seaport[i]).call{value: value}(data[i]);
        }
        for (uint256 i = 1; i < N; i++) {
            if (ok[i] != ok[0] || (ok[0] && keccak256(ret[i]) != keccak256(ret[0]))) {
                return (ok, string.concat("outcome: ", names[i], " vs ", names[0]));
            }
        }
        // Same source, two compilers: revert data must match too. Orders built for one build
        // name its own tokens, so errors that carry an order hash compare by selector.
        for (uint256 j; j < 2; j++) {
            uint256 i = j + 2;
            bool same = keccak256(data[i]) == keccak256(data[j])
                ? keccak256(ret[i]) == keccak256(ret[j])
                : bytes4(ret[i]) == bytes4(ret[j]);
            if (!same) return (ok, string.concat("revert data: ", names[i], " vs ", names[j]));
        }
    }

    function _compare(bytes[N] memory data, uint256 value) internal returns (bool[N] memory ok) {
        string memory diff;
        (ok, diff) = _run(data, value);
        assertEq(diff, "");
    }

    function _same(bytes memory data) internal returns (bool[N] memory) {
        return _compare([data, data, data, data], 0);
    }

    /// ETH for an ERC721, with a tip to a second recipient, signed by the offerer: Seaport's
    /// basic.spec.ts order shape.
    function _basic(uint256 i, Basic memory b, address offerer) internal view returns (bytes memory fill, bytes memory cancel) {
        OrderComponents memory c = _components(i, b, offerer);
        fill = abi.encodeCall(ConsiderationInterface.fulfillBasicOrder, (_params(i, b, c, _sign(i, b.key, c))));
        OrderComponents[] memory cs = new OrderComponents[](1);
        cs[0] = c;
        cancel = abi.encodeCall(ConsiderationInterface.cancel, (cs));
    }

    function _components(uint256 i, Basic memory b, address offerer) internal view returns (OrderComponents memory c) {
        OfferItem[] memory offer = new OfferItem[](1);
        offer[0] = OfferItem(ItemType.ERC721, address(nft[i]), b.tokenId, 1, 1);
        ConsiderationItem[] memory cons = new ConsiderationItem[](2);
        cons[0] = ConsiderationItem(ItemType.NATIVE, address(0), 0, b.price, b.price, payable(offerer));
        cons[1] = ConsiderationItem(ItemType.NATIVE, address(0), 0, b.tip, b.tip, payable(address(0xbeef)));
        c.offerer = offerer;
        c.offer = offer;
        c.consideration = cons;
        c.orderType = OrderType.FULL_OPEN;
        c.startTime = block.timestamp - (b.start % 1000);
        c.endTime = c.startTime + b.duration;
        c.salt = b.salt;
        c.counter = seaport[i].getCounter(offerer);
    }

    function _sign(uint256 i, uint256 key, OrderComponents memory c) internal view returns (bytes memory) {
        (, bytes32 domain,) = seaport[i].information();
        bytes32 orderHash = seaport[i].getOrderHash(c);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, keccak256(abi.encodePacked(bytes2(0x1901), domain, orderHash)));
        return abi.encodePacked(r, s, v);
    }

    function _params(uint256 i, Basic memory b, OrderComponents memory c, bytes memory signature)
        internal
        view
        returns (BasicOrderParameters memory p)
    {
        AdditionalRecipient[] memory extra = new AdditionalRecipient[](1);
        extra[0] = AdditionalRecipient(b.tip, payable(address(0xbeef)));
        p.considerationAmount = b.price;
        p.offerer = payable(c.offerer);
        p.offerToken = address(nft[i]);
        p.offerIdentifier = b.tokenId;
        p.offerAmount = 1;
        p.basicOrderType = BasicOrderType.ETH_TO_ERC721_FULL_OPEN;
        p.startTime = c.startTime;
        p.endTime = c.endTime;
        p.salt = b.salt;
        p.totalOriginalAdditionalRecipients = 1;
        p.additionalRecipients = extra;
        p.signature = signature;
    }

    /// Mints the token to the offerer in every build and builds its order there.
    function _prepare(Basic memory b) internal returns (address offerer, bytes[N] memory fills, bytes[N] memory cancels) {
        b.key = bound(b.key, 1, 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364140);
        offerer = vm.addr(b.key);
        for (uint256 i; i < N; i++) {
            try nft[i].ownerOf(b.tokenId) returns (address) {} catch {
                nft[i].mint(offerer, b.tokenId);
            }
            vm.prank(offerer);
            nft[i].setApprovalForAll(address(seaport[i]), true);
            (fills[i], cancels[i]) = _basic(i, b, offerer);
        }
    }
}

contract SeaportDiff is Builds {
    // Seaport's own vectors: basic.spec.ts pays 10 ether to the seller and 1 ether each to a
    // zone and an owner; FulfillBasicOrderTest fuzzes uint128 payments.
    uint128[] public fixturePrice = [10 ether, 1 ether, 1, type(uint128).max];
    uint128[] public fixtureTip = [1 ether, 0, 1];
    // Words with only the high bits set, or one bit past a packed field, as dirty storage would
    // leave them.
    uint256[] public fixtureValue = [type(uint256).max, 1 << 255, 1 << 160, 1 << 120, 1 << 8, 2];

    function setUp() public {
        _deploy();
        vm.warp(1_700_000_000);
    }

    receive() external payable {}

    function testFuzz_views(address who) public {
        _same(abi.encodeCall(ConsiderationInterface.name, ()));
        _same(abi.encodeCall(ConsiderationInterface.getCounter, (who)));
        _same(abi.encodeCall(ConsiderationInterface.getOrderStatus, (keccak256(abi.encode(who)))));
    }

    /// Getters over arbitrary stored words, as forge equivalence assumes them: every slot a
    /// getter reads is first set to the same value in every build.
    function testFuzz_storedWords(address who, bytes32 orderHash, uint256 value) public {
        bytes[3] memory calls = [
            abi.encodeCall(ConsiderationInterface.getCounter, (who)),
            abi.encodeCall(ConsiderationInterface.getOrderStatus, (orderHash)),
            abi.encodeCall(ConsiderationInterface.getContractOffererNonce, (who))
        ];
        for (uint256 c; c < calls.length; c++) {
            for (uint256 i; i < N; i++) {
                vm.record();
                (bool ok,) = address(seaport[i]).call(calls[c]);
                ok;
                (bytes32[] memory reads,) = vm.accesses(address(seaport[i]));
                for (uint256 r; r < reads.length; r++) {
                    vm.store(address(seaport[i]), reads[r], bytes32(value));
                }
            }
            _same(calls[c]);
        }
    }

    /// Hashing any well-formed order.
    function testFuzz_getOrderHash(OrderComponents memory c) public {
        vm.assume(c.offer.length <= 4 && c.consideration.length <= 4);
        for (uint256 i; i < c.offer.length; i++) {
            c.offer[i].itemType = ItemType(uint8(c.offer[i].itemType) % 6);
        }
        for (uint256 i; i < c.consideration.length; i++) {
            c.consideration[i].itemType = ItemType(uint8(c.consideration[i].itemType) % 6);
        }
        c.orderType = OrderType(uint8(c.orderType) % 5);
        _same(abi.encodeCall(ConsiderationInterface.getOrderHash, (c)));
    }

    /// Raw calldata into every entry point: dispatch, decoding and revert paths.
    function testFuzz_rawCalldata(bytes4 selector, bytes memory tail) public {
        _same(abi.encodePacked(selector, tail));
    }

    function testFuzz_incrementCounter(uint8 times) public {
        for (uint256 t; t < times % 4; t++) {
            _same(abi.encodeCall(ConsiderationInterface.incrementCounter, ()));
        }
        _same(abi.encodeCall(ConsiderationInterface.getCounter, (address(this))));
    }

    function testFuzz_basicOrder(uint256 key, uint256 tokenId, uint128 price, uint128 tip, uint256 salt, uint32 start, uint32 duration, uint8 extra, bool cancel) public {
        _basicOrder(Basic(key, tokenId, price, tip, salt, start, duration, extra, cancel));
    }

    /// basic.spec.ts's first order: it must fill everywhere, so the fuzz test is not vacuous.
    function test_basicOrderVector() public {
        bool ok = _basicOrder(Basic(0xA11CE, 1, 10 ether, 1 ether, 7, 0, 1 days, 0, false));
        assertTrue(ok, "the vector fills");
    }

    /// Found by forge equivalence on the reference builds (docs/SOLC_DIVERGENCE.md, CODEGEN-030):
    /// a forged out-of-range entry in _BasicOrderToOrderType (slot 6) is checked where it is
    /// loaded by the compiler under test and where it is used by solc, so the two report
    /// different errors for an unpaid ETH order.
    function test_storedEnumCheckOrder() public {
        Basic memory b = Basic(0xA11CE, 1, 10 ether, 1 ether, 7, 0, 1 days, 0, false);
        (, bytes[N] memory fills,) = _prepare(b);
        bytes32 slot = keccak256(abi.encode(uint256(BasicOrderType.ETH_TO_ERC721_FULL_OPEN), uint256(6)));
        bytes[2] memory expected = [
            abi.encodeWithSignature("InvalidMsgValue(uint256)", 0),
            abi.encodeWithSignature("Panic(uint256)", 0x21)
        ];
        uint256[2] memory builds = [uint256(1), 3];
        for (uint256 k; k < 2; k++) {
            uint256 i = builds[k];
            assertEq(vm.load(address(seaport[i]), slot), bytes32(0), "OrderType.FULL_OPEN");
            vm.store(address(seaport[i]), slot, bytes32(uint256(6)));
            (bool ok, bytes memory ret) = address(seaport[i]).call(fills[i]);
            assertFalse(ok, names[i]);
            assertEq(ret, expected[k], names[i]);
        }
    }

    function _basicOrder(Basic memory b) internal returns (bool) {
        uint256 total = uint256(b.price) + b.tip;
        vm.deal(address(this), (2 * total + b.extra) * N);
        (address offerer, bytes[N] memory fills, bytes[N] memory cancels) = _prepare(b);
        vm.assume(offerer.code.length == 0 && offerer != address(this));
        if (b.cancel) {
            vm.startPrank(offerer);
            _compare(cancels, 0);
            vm.stopPrank();
        }
        uint256 before = offerer.balance;
        bool[N] memory ok = _compare(fills, total + b.extra);
        for (uint256 i; i < N; i++) {
            assertEq(nft[i].ownerOf(b.tokenId), ok[i] ? address(this) : offerer, names[i]);
        }
        assertEq(offerer.balance, before + (ok[0] ? uint256(b.price) * N : 0), "the offerer is paid once per build");
        // Filling the same order again must fail the same way everywhere.
        _compare(fills, total);
        return ok[0];
    }
}

/// Random sequences of fills, cancellations and counter bumps, applied in lockstep to every
/// build; the invariant is that no step told the builds apart.
contract Lockstep is Builds {
    string public diff;
    uint256 public steps;

    constructor() {
        _deploy();
    }

    receive() external payable {}

    function _note(string memory d) internal {
        steps++;
        if (bytes(diff).length == 0 && bytes(d).length != 0) diff = d;
    }

    function incrementCounter(uint8 who) external {
        address a = vm.addr(uint256(who % 4) + 1);
        bytes memory data = abi.encodeCall(ConsiderationInterface.incrementCounter, ());
        vm.prank(a);
        (, string memory d0) = _runAs(a, [data, data, data, data], 0);
        _note(d0);
    }

    function fill(uint8 who, uint8 tokenId, uint128 price, uint128 tip, uint8 salt, bool cancel) external {
        _fill(Basic(uint256(who % 4) + 1, tokenId, price % 100 ether, tip % 1 ether, salt, 0, 1 days, 0, cancel));
    }

    function _fill(Basic memory b) internal {
        vm.deal(address(this), (uint256(b.price) + b.tip) * N);
        (address offerer, bytes[N] memory fills, bytes[N] memory cancels) = _prepare(b);
        if (b.cancel) _step(offerer, cancels, 0);
        _step(address(this), fills, uint256(b.price) + b.tip);
    }

    function _step(address who, bytes[N] memory data, uint256 value) internal {
        (, string memory d) = _runAs(who, data, value);
        _note(d);
    }

    function _runAs(address who, bytes[N] memory data, uint256 value) internal returns (bool[N] memory ok, string memory d) {
        vm.startPrank(who);
        (ok, d) = _run(data, value);
        vm.stopPrank();
    }

    function counters() external returns (bool same) {
        same = true;
        for (uint256 k = 1; k <= 4; k++) {
            uint256 c0 = seaport[0].getCounter(vm.addr(k));
            for (uint256 i = 1; i < N; i++) {
                same = same && seaport[i].getCounter(vm.addr(k)) == c0;
            }
        }
    }
}

contract SeaportLockstep is Test {
    Lockstep internal h;

    function setUp() public {
        vm.warp(1_700_000_000);
        h = new Lockstep();
        targetContract(address(h));
    }

    function invariant_buildsAgree() public {
        assertEq(h.diff(), "");
        assertTrue(h.counters(), "counters");
    }
}
