//@ compile-flags: -O gas
//@ run-call: Consideration::getCounter 0x0000000000000000000000000000000000000001; constructor=[0x0000000000000000000000000000000000000002] => 0
//@ run-call: Consideration::probe [], 0x0000000000000000000000000000000000000001; constructor=[0x0000000000000000000000000000000000000002] => 0, false
//@ run-call: Consideration::probe [((0x0000000000000000000000000000000000000003,0x0000000000000000000000000000000000000004,[(1,2)],[(3,4)],0,5,6)),((0x0000000000000000000000000000000000000007,0x0000000000000000000000000000000000000008,[],[],0,9,10))], 0x0000000000000000000000000000000000000001; constructor=[0x0000000000000000000000000000000000000002] => 0, false
// Reduced from `testdata/Seaport.sol`, whose
// `_validateOrdersAndPrepareToFulfill` reached the backend's planned-edge
// spill loop with a value that is live across an edge into an
// already-emitted block while it is neither on this block's stack nor
// stored. Emitting bytecode for it aborted an assertion-enabled build with
// "lives across the edge ... with no home"; the block has nothing to store
// there, so the store obligation is not its own.
// `probe` calls that function directly so the block the condition fires for
// also runs; its results match solc for 0, 1, 2 and 5 orders at osaka and
// homestead, both optimized and unoptimized.
// `-Zassert-planned-edge-spill-home` turns the condition back into a panic.
interface AmountDerivationErrors {
    /**
     */
}
interface ConduitControllerInterface {
    /**
     */
    function getConduitCodeHashes()
        external
        returns (bytes32 creationCodeHash, bytes32 runtimeCodeHash);
}
/*
 */
uint256 constant Offset_fulfillAdvancedOrder_criteriaResolvers = 0x20;
uint256 constant Offset_matchOrders_fulfillments = 0x20;
uint256 constant Offset_matchAdvancedOrders_criteriaResolvers = 0x20;
uint256 constant Offset_matchAdvancedOrders_fulfillments = 0x40;
uint256 constant ReceivedItem_recipient_offset = 0x80;
uint256 constant ConsiderationItem_recipient_offset = 0xa0;
uint256 constant AdvancedOrder_extraData_offset = 0x80;
uint256 constant OneWord_0 = 0x20;
uint256 constant OneWordShift_0 = 0x5;
uint256 constant NonMatchSelector_MagicMask =
    (0x4000000000000000000000000000000000000000000000000000000000);
uint256 constant NonMatchSelector_InvalidErrorValue =
    (0x4000000000000000000000000000000000000000000000000000000001);
uint256 constant OrderParameters_totalOriginalConsiderationItems_offset = (
    0x0140
);
enum OrderType {
    CONTRACT
}
type CalldataPointer is uint256;
type MemoryPointer is uint256;
using CalldataPointerLib for CalldataPointer global;
using MemoryPointerLib for MemoryPointer global;
using CalldataReaders for CalldataPointer global;
using MemoryWriters for MemoryPointer global;
CalldataPointer constant CalldataStart = CalldataPointer.wrap(0x04);
uint256 constant OffsetOrLengthMask = 0xffffffff;
library CalldataPointerLib {
    function pptrOffset(
        CalldataPointer cdPtr,
        uint256 headOffset
    ) internal pure returns (CalldataPointer cdPtrChild) {
        cdPtrChild = cdPtr.offset(
            cdPtr.offset(headOffset).readUint256() & OffsetOrLengthMask
        );
    }
    function pptr(
        CalldataPointer cdPtr
    ) internal pure returns (CalldataPointer cdPtrChild) {
    }
    function offset(
        CalldataPointer cdPtr,
        uint256 _offset
    ) internal pure returns (CalldataPointer cdPtrNext) {
    }
}
library MemoryPointerLib {
    function offset(
        MemoryPointer mPtr,
        uint256 _offset
    ) internal pure returns (MemoryPointer mPtrNext) {
    }
}
library CalldataReaders {
    function readUint256(
        CalldataPointer cdPtr
    ) internal pure returns (uint256 value) {
    }
}
library MemoryWriters {
    function write(MemoryPointer mPtr, MemoryPointer valuePtr) internal pure {
    }
}
interface SignatureVerificationErrors {
}
interface TokenTransferrerErrors {
}
interface FulfillmentApplicationErrors {
}
contract AmountDeriver is AmountDerivationErrors {
    function _locateCurrentAmount(
        uint256 startAmount,
        uint256 endAmount,
        uint256 startTime,
        uint256 endTime,
        bool roundUp
    ) internal view returns (uint256 amount) {
    }
    function _getFraction(
        uint256 numerator,
        uint256 denominator,
        uint256 value
    ) internal pure returns (uint256 newValue) {
    }
}
struct OrderComponents {
    uint256 counter;
}
struct OfferItem {
    uint256 startAmount;
    uint256 endAmount;
}
struct ConsiderationItem {
    uint256 startAmount;
    uint256 endAmount;
}
struct SpentItem {
    uint256 amount;
}
struct ReceivedItem {
    uint256 amount;
}
struct BasicOrderParameters {
    bytes signature; // 0x244
}
struct OrderParameters {
    address offerer; // 0x00
    address zone; // 0x20
    OfferItem[] offer; // 0x40
    ConsiderationItem[] consideration; // 0x60
    OrderType orderType; // 0x80
    uint256 startTime; // 0xa0
    uint256 endTime; // 0xc0
}
struct Order {
    bytes signature;
}
struct AdvancedOrder {
    OrderParameters parameters;
}
struct CriteriaResolver {
    bytes32[] criteriaProof;
}
struct Fulfillment {
    FulfillmentComponent[] considerationComponents;
}
struct FulfillmentComponent {
    uint256 itemIndex;
}
struct Execution {
    bytes32 conduitKey;
}
interface ConsiderationEventsAndErrors {
    event OrderFulfilled(
        bytes32 orderHash,
        address indexed offerer,
        address indexed zone,
        address recipient,
        SpentItem[] offer,
        ReceivedItem[] consideration
    );
}
/**
 */
interface ConsiderationInterface {
    function fulfillBasicOrder(
        BasicOrderParameters calldata parameters
    ) external payable returns (bool fulfilled);
    /**
     */
    function fulfillOrder(
        Order calldata order,
        bytes32 fulfillerConduitKey
    ) external payable returns (bool fulfilled);
    /**
     */
    function fulfillAdvancedOrder(
        AdvancedOrder calldata advancedOrder,
        CriteriaResolver[] calldata criteriaResolvers,
        bytes32 fulfillerConduitKey,
        address recipient
    ) external payable returns (bool fulfilled);
    /**
     */
    function matchOrders(
        Order[] calldata orders,
        Fulfillment[] calldata fulfillments
    ) external payable returns (Execution[] memory executions);
    /**
     */
    function matchAdvancedOrders(
        AdvancedOrder[] calldata orders,
        CriteriaResolver[] calldata criteriaResolvers,
        Fulfillment[] calldata fulfillments,
        address recipient
    ) external payable returns (Execution[] memory executions);
    /**
     */
    function cancel(
        OrderComponents[] calldata orders
    ) external returns (bool cancelled);
    function validate(
        Order[] calldata orders
    ) external returns (bool validated);
    function incrementCounter() external returns (uint256 newCounter);
    function fulfillBasicOrder_efficient_6GL6yc(
        BasicOrderParameters calldata parameters
    ) external payable returns (bool fulfilled);
    /**
     */
    function getOrderHash(
        OrderComponents calldata order
    ) external view returns (bytes32 orderHash);
    function getOrderStatus(
        bytes32 orderHash
    )
        external
        returns (
            bool isValidated,
            bool isCancelled,
            uint256 totalFilled,
            uint256 totalSize
        );
    function getCounter(
        address offerer
    ) external view returns (uint256 counter);
    /**
     */
    function information()
        external
        returns (
            string memory version,
            bytes32 domainSeparator,
            address conduitController
        );
    function getContractOffererNonce(
        address contractOfferer
    ) external view returns (uint256 nonce);
    /**
     */
}
contract ConsiderationDecoder {
    function _decodeBytes(
        CalldataPointer cdPtrLength
    ) internal pure returns (MemoryPointer mPtrLength) {
    }
    function _decodeAdvancedOrder(
        CalldataPointer cdPtr
    ) internal pure returns (MemoryPointer mPtr) {
        mPtr.offset(AdvancedOrder_extraData_offset).write(
            _decodeBytes(cdPtr.pptrOffset(AdvancedOrder_extraData_offset))
        );
    }
    function _decodeOrderAsAdvancedOrder(
        CalldataPointer cdPtr
    ) internal pure returns (MemoryPointer mPtr) {
    }
    function _decodeOrdersAsAdvancedOrders(
        CalldataPointer cdPtrLength
    ) internal pure returns (MemoryPointer mPtrLength) {
    }
    function _decodeCriteriaResolvers(
        CalldataPointer cdPtrLength
    ) internal pure returns (MemoryPointer mPtrLength) {
    }
    function _decodeAdvancedOrders(
        CalldataPointer cdPtrLength
    ) internal pure returns (MemoryPointer mPtrLength) {
    }
    function _decodeFulfillments(
        CalldataPointer cdPtrLength
    ) internal pure returns (MemoryPointer mPtrLength) {
    }
    function _toAdvancedOrderReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    )
        internal
        returns (
            function(CalldataPointer)
                returns (AdvancedOrder memory) outFn
        )
    {
    }
    function _toCriteriaResolversReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    )
        internal
        returns (
            function(CalldataPointer)
                returns (CriteriaResolver[] memory) outFn
        )
    {
    }
    function _toAdvancedOrdersReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    )
        internal
        returns (
            function(CalldataPointer)
                returns (AdvancedOrder[] memory) outFn
        )
    {
    }
    function _toFulfillmentsReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    )
        internal
        returns (
            function(CalldataPointer)
                returns (Fulfillment[] memory) outFn
        )
    {
    }
}
contract LowLevelHelpers {
    function _substituteCallerForEmptyRecipient(
        address recipient
    ) internal view returns (address updatedRecipient) {
    }
    function _readAO( //~ WARN: function state mutability can be restricted to pure
        AdvancedOrder[] memory a,
        uint256 i
    ) internal returns (AdvancedOrder memory) {
        return a[(i >> 5) - 1];
    }
    function _getReadAdvancedOrderByOffset() //~ WARN: function state mutability can be restricted to pure
        internal
        returns (
            function(AdvancedOrder[] memory, uint256)
                returns (AdvancedOrder memory) fn2
        )
    {
        fn2 = _readAO;
    }
    function _runTimeConstantTrue() internal pure returns (bool) {
    }
    function _runTimeConstantFalse() internal pure returns (bool) {
    }
}
contract TokenTransferrer is TokenTransferrerErrors {
}
contract FulfillmentApplier is FulfillmentApplicationErrors {
}
contract SignatureVerification is SignatureVerificationErrors, LowLevelHelpers {
}
contract ConsiderationBase is
    ConsiderationDecoder,
    ConsiderationEventsAndErrors
{
    bytes32 internal immutable _NAME_HASH;
    bytes32 internal immutable _VERSION_HASH;
    bytes32 internal immutable _EIP_712_DOMAIN_TYPEHASH;
    bytes32 internal immutable _OFFER_ITEM_TYPEHASH;
    bytes32 internal immutable _CONSIDERATION_ITEM_TYPEHASH;
    constructor(address) {
        (
            _NAME_HASH,
            _VERSION_HASH,
            _EIP_712_DOMAIN_TYPEHASH,
            _OFFER_ITEM_TYPEHASH,
            _CONSIDERATION_ITEM_TYPEHASH,
        ) = _deriveTypehashes();
    }
    function _deriveTypehashes() //~ WARN: function state mutability can be restricted to pure
        internal
        returns (
            bytes32 nameHash,
            bytes32 versionHash,
            bytes32 eip712DomainTypehash,
            bytes32 offerItemTypehash,
            bytes32 considerationItemTypehash,
            bytes32 orderTypehash
        )
    {
        bytes memory offerItemTypeString = bytes(
            "OfferItem("
        );
    }
}
contract ZoneInteraction is
    LowLevelHelpers
{
    function _assertRestrictedAdvancedOrderAuthorization(
        AdvancedOrder memory advancedOrder,
        bytes32[] memory orderHashes,
        bytes32 orderHash,
        uint256 orderIndex
    ) internal {
    }
    function _assertRestrictedAdvancedOrderValidity(
        AdvancedOrder memory advancedOrder,
        bytes32[] memory orderHashes,
        bytes32 orderHash
    ) internal {
    }
}
contract GettersAndDerivers is ConsiderationBase {
    constructor(
        address conduitController
    ) ConsiderationBase(conduitController) {}
}
contract Assertions is
    GettersAndDerivers,
    TokenTransferrerErrors
{
    constructor(
        address conduitController
    ) GettersAndDerivers(conduitController) {}
}
contract Verifiers is Assertions, SignatureVerification {
    constructor(address conduitController) Assertions(conduitController) {}
}
contract Executor is Verifiers, TokenTransferrer {
    constructor(address conduitController) Verifiers(conduitController) {}
}
contract OrderValidator is Executor, ZoneInteraction {
    constructor(address conduitController) Executor(conduitController) {}
    function _validateOrder(
        AdvancedOrder memory advancedOrder,
        bool revertOnInvalid
    )
        internal
        returns (bytes32 orderHash, uint256 numerator, uint256 denominator)
    {
    }
}
contract BasicOrderFulfiller is OrderValidator {
    constructor(address conduitController) OrderValidator(conduitController) {}
}
contract OrderFulfiller is
    BasicOrderFulfiller,
    AmountDeriver
{
    constructor(
        address conduitController
    ) BasicOrderFulfiller(conduitController) {}
    function _validateAndFulfillAdvancedOrder(
        AdvancedOrder memory advancedOrder,
        CriteriaResolver[] memory criteriaResolvers,
        bytes32 fulfillerConduitKey,
        address recipient
    ) internal returns (bool) {
        OrderParameters memory orderParameters = advancedOrder.parameters;
        OrderType orderType = orderParameters.orderType;
        (
            bytes32 orderHash,
            uint256 fillNumerator,
        ) = _validateOrder(advancedOrder, _runTimeConstantTrue());
        bytes32[] memory orderHashes = new bytes32[](1);
        if (orderType != OrderType.CONTRACT) {
            _assertRestrictedAdvancedOrderAuthorization(
                advancedOrder,
                orderHashes,
                orderHash,
                0
            );
        }
        _assertRestrictedAdvancedOrderValidity(
            advancedOrder,
            orderHashes,
            orderHash
        );
        _emitOrderFulfilledEvent(
            orderHash,
            orderParameters.offerer,
            orderParameters.zone,
            recipient,
            orderParameters.offer,
            orderParameters.consideration
        );
    }
    function _emitOrderFulfilledEvent(
        bytes32 orderHash,
        address offerer,
        address zone,
        address recipient,
        OfferItem[] memory offer,
        ConsiderationItem[] memory consideration
    ) internal {
        SpentItem[] memory spentItems;
        ReceivedItem[] memory receivedItems;
        emit OrderFulfilled(
            orderHash,
            offerer,
            zone,
            recipient,
            spentItems,
            receivedItems
        );
    }
}
contract OrderCombiner is OrderFulfiller, FulfillmentApplier {
    constructor(address conduitController) OrderFulfiller(conduitController) {}
    function _validateOrdersAndPrepareToFulfill(
        AdvancedOrder[] memory advancedOrders,
        CriteriaResolver[] memory criteriaResolvers,
        bool revertOnInvalid,
        uint256 maximumFulfilled,
        address recipient
    ) internal returns (bytes32[] memory orderHashes, bool containsNonOpen) {
        uint256 terminalMemoryOffset;
        {
            uint256 invalidNativeOfferItemErrorBuffer;
            assembly {
                invalidNativeOfferItemErrorBuffer := and(
                    NonMatchSelector_MagicMask,
                    calldataload(0)
                )
            }
            unchecked {
                uint256 totalOrders = advancedOrders.length;
                terminalMemoryOffset = (totalOrders + 1) << OneWordShift_0;
                for (
                    uint256 i = OneWord_0;
                    i < terminalMemoryOffset;
                    i += OneWord_0
                ) {
                    AdvancedOrder memory advancedOrder = (
                        _getReadAdvancedOrderByOffset()(advancedOrders, i)
                    );
                    (
                        bytes32 orderHash,
                        uint256 numerator,
                        uint256 denominator
                    ) = _validateOrder(advancedOrder, revertOnInvalid);
                    if (numerator == 0) {
                        continue;
                    }
                    uint256 startTime = advancedOrder.parameters.startTime;
                    uint256 endTime = advancedOrder.parameters.endTime;
                    {
                        OrderType orderType = (
                            advancedOrder.parameters.orderType
                        );
                        assembly {
                            containsNonOpen := or(
                                containsNonOpen,
                                gt(orderType, 1)
                            )
                        }
                    }
                    OfferItem[] memory offer = advancedOrder.parameters.offer;
                    uint256 totalOfferItems = offer.length;
                    for (uint256 j = 0; j < totalOfferItems; ++j) {
                        OfferItem memory offerItem = offer[j];
                        uint256 endAmount = _getFraction(
                            numerator,
                            denominator,
                            offerItem.endAmount
                        );
                        uint256 currentAmount = _locateCurrentAmount(
                            offerItem.startAmount,
                            endAmount,
                            startTime,
                            endTime,
                            _runTimeConstantFalse() // round down
                        );
                    }
                    ConsiderationItem[] memory consideration = (
                        advancedOrder.parameters.consideration
                    );
                    uint256 totalConsiderationItems = consideration.length;
                    for (uint256 j = 0; j < totalConsiderationItems; ++j) {
                        ConsiderationItem memory considerationItem = (
                            consideration[j]
                        );
                        uint256 endAmount = _getFraction(
                            numerator,
                            denominator,
                            considerationItem.endAmount
                        );
                        if (
                            considerationItem.startAmount ==
                            considerationItem.endAmount
                        ) {
                            considerationItem.startAmount = _getFraction(
                                numerator,
                                denominator,
                                considerationItem.startAmount
                            );
                        }
                        uint256 currentAmount = (
                            _locateCurrentAmount(
                                considerationItem.startAmount,
                                endAmount,
                                startTime,
                                endTime,
                                _runTimeConstantTrue() // round up
                            )
                        );
                        assembly {
                            let considerationItemRecipientPtr := add(
                                considerationItem,
                                ConsiderationItem_recipient_offset
                            )
                            mstore(
                                add(
                                    considerationItem,
                                    ReceivedItem_recipient_offset
                                ),
                                mload(considerationItemRecipientPtr)
                            )
                        }
                    }
                }
            }
            if (
                invalidNativeOfferItemErrorBuffer ==
                NonMatchSelector_InvalidErrorValue
            ) {
            }
        }
    }
    function _matchAdvancedOrders(
        AdvancedOrder[] memory advancedOrders,
        CriteriaResolver[] memory criteriaResolvers,
        Fulfillment[] memory fulfillments,
        address recipient
    ) internal returns (Execution[] memory /* executions */) {
        bool revertOnInvalid = _runTimeConstantTrue();
        (
            bytes32[] memory orderHashes,
            bool containsNonOpen
        ) = _validateOrdersAndPrepareToFulfill(
                advancedOrders,
                criteriaResolvers,
                revertOnInvalid,
                advancedOrders.length,
                recipient
            );
            _fulfillAdvancedOrders(
                advancedOrders,
                fulfillments,
                orderHashes,
                recipient,
                containsNonOpen
            );
    }
    function _fulfillAdvancedOrders(
        AdvancedOrder[] memory advancedOrders,
        Fulfillment[] memory fulfillments,
        bytes32[] memory orderHashes,
        address recipient,
        bool containsNonOpen
    ) internal returns (Execution[] memory executions) {
    }
    function probe(
        AdvancedOrder[] memory orders,
        address r
    ) public returns (uint256, bool) {
        (bytes32[] memory h, bool c) = _validateOrdersAndPrepareToFulfill(
            orders,
            new CriteriaResolver[](0),
            true,
            orders.length,
            r
        );
        return (h.length, c);
    }
}
contract Consideration is ConsiderationInterface, OrderCombiner {
    constructor(address conduitController) OrderCombiner(conduitController) {}
    function fulfillBasicOrder(
        BasicOrderParameters calldata
    ) external payable override returns (bool fulfilled) {
    }
    function fulfillBasicOrder_efficient_6GL6yc(
        BasicOrderParameters calldata
    ) external payable override returns (bool fulfilled) {
    }
    function fulfillOrder(
        Order calldata,
        bytes32 fulfillerConduitKey
    ) external payable override returns (bool fulfilled) {
        fulfilled = _validateAndFulfillAdvancedOrder(
            _toAdvancedOrderReturnType(_decodeOrderAsAdvancedOrder)(
                CalldataStart.pptr()
            ),
            new CriteriaResolver[](0), // No criteria resolvers supplied.
            fulfillerConduitKey,
            msg.sender
        );
    }
    function fulfillAdvancedOrder(
        AdvancedOrder calldata,
        CriteriaResolver[] calldata,
        bytes32 fulfillerConduitKey,
        address recipient
    ) external payable override returns (bool fulfilled) {
        fulfilled = _validateAndFulfillAdvancedOrder(
            _toAdvancedOrderReturnType(_decodeAdvancedOrder)(
                CalldataStart.pptr()
            ),
            _toCriteriaResolversReturnType(_decodeCriteriaResolvers)(
                CalldataStart.pptrOffset(
                    Offset_fulfillAdvancedOrder_criteriaResolvers
                )
            ),
            fulfillerConduitKey,
            _substituteCallerForEmptyRecipient(recipient)
        );
    }
    function matchOrders(
        Order[] calldata,
        Fulfillment[] calldata
    ) external payable override returns (Execution[] memory /* executions */) {
            _matchAdvancedOrders(
                _toAdvancedOrdersReturnType(_decodeOrdersAsAdvancedOrders)(
                    CalldataStart.pptr()
                ),
                new CriteriaResolver[](0), // No criteria resolvers supplied.
                _toFulfillmentsReturnType(_decodeFulfillments)(
                    CalldataStart.pptrOffset(Offset_matchOrders_fulfillments)
                ),
                msg.sender
            );
    }
    function matchAdvancedOrders(
        AdvancedOrder[] calldata,
        CriteriaResolver[] calldata,
        Fulfillment[] calldata,
        address recipient
    ) external payable override returns (Execution[] memory /* executions */) {
            _matchAdvancedOrders(
                _toAdvancedOrdersReturnType(_decodeAdvancedOrders)(
                    CalldataStart.pptr()
                ),
                _toCriteriaResolversReturnType(_decodeCriteriaResolvers)(
                    CalldataStart.pptrOffset(
                        Offset_matchAdvancedOrders_criteriaResolvers
                    )
                ),
                _toFulfillmentsReturnType(_decodeFulfillments)(
                    CalldataStart.pptrOffset(
                        Offset_matchAdvancedOrders_fulfillments
                    )
                ),
                _substituteCallerForEmptyRecipient(recipient)
            );
    }
    function cancel(
        OrderComponents[] calldata orders
    ) external override returns (bool cancelled) {
    }
    function validate(
        Order[] calldata
    ) external override returns (bool /* validated */) {
    }
    function incrementCounter() external override returns (uint256 newCounter) {
    }
    function getOrderHash(
        OrderComponents calldata
    ) external view override returns (bytes32 orderHash) {
    }
    function getOrderStatus(
        bytes32 orderHash
    )
        external
        returns (
            bool isValidated,
            bool isCancelled,
            uint256 totalFilled,
            uint256 totalSize
        )
    {
    }
    function getCounter(
        address offerer
    ) external view override returns (uint256 counter) {
    }
    function information()
        external
        returns (
            string memory version,
            bytes32 domainSeparator,
            address conduitController
        )
    {
    }
    function getContractOffererNonce(
        address contractOfferer
    ) external view override returns (uint256 nonce) {
    }
}