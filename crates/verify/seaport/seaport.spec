# Seaport 1.6 specs for solar-verify.
#
# This file is applied to an unmodified Seaport checkout (ProjectOpenSea/seaport
# @ 1170198) by apply_specs.py, which writes a copy with each block below added
# as NatSpec tags in front of the named contract or function. Nothing else in
# the copy changes, and no spec here is a loop invariant: loop invariants are
# inferred by solar-verify.
#
# Format: a header line names the target, the lines after it (until the next
# header) are tags written without the leading "///".
#   == contract <Contract>
#   == function <Contract>.<name>[(<parameter types>)]
# Lines starting with "#" are comments. "\" at the end of a line joins it with
# the next one.
#
# "@define NAME body" and "@define NAME(x) body" declare shorthands, expanded
# textually (repeatedly) before the tags are written; "\n" in a body starts a
# new tag. The main ones:
#   GUARD   the reentrancy guard is set (transient storage, or the sstore
#           sentinel when transient storage is not in use)
#   ST(h)   _orderStatus[h]

@define ST(h) _orderStatus[h]
@define GUARD ((_tstoreInitialSupport || __raw[0x929eee14] == 0) ? __traw[0x929eee14] != 0 : __raw[0x929eee14] > 1)
@define COUNTERS_KEPT forall address a :: _counters[a] == old(_counters[a])
@define FILLS_KEPT forall bytes32 h :: ST(h).numerator == old(ST(h).numerator) && ST(h).denominator == old(ST(h).denominator)
@define STATUS_KEPT forall bytes32 h :: ST(h).isValidated == old(ST(h).isValidated) && ST(h).isCancelled == old(ST(h).isCancelled) && ST(h).numerator == old(ST(h).numerator) && ST(h).denominator == old(ST(h).denominator)
@define NONCES_KEPT forall address a :: _contractNonces[a] == old(_contractNonces[a])
# Fulfillment: cancellation is final and blocks fills; validation is kept;
# the filled fraction numerator/denominator never decreases.
@define FILL_RULES \
@custom:ensures forall bytes32 h :: ST(h).isCancelled == old(ST(h).isCancelled)\n\
@custom:ensures forall bytes32 h :: old(ST(h).isCancelled) ==> ST(h).numerator == old(ST(h).numerator) && ST(h).denominator == old(ST(h).denominator)\n\
@custom:ensures forall bytes32 h :: old(ST(h).isValidated) ==> ST(h).isValidated\n\
@custom:ensures forall bytes32 h :: old(ST(h).denominator) != 0 ==> old(ST(h).numerator) * ST(h).denominator <= ST(h).numerator * old(ST(h).denominator)\n\
@custom:ensures COUNTERS_KEPT\n\
@custom:ensures !GUARD
# Order fulfillment returns every native token it holds at the end to the
# caller, so Seaport keeps no native balance.
@define RETAINS_NOTHING @custom:ensures address(this).balance == 0

== contract Consideration
# Order status: a filled fraction never exceeds one, and a cancelled order is
# never also marked validated.
@custom:invariant forall bytes32 h :: ST(h).numerator <= ST(h).denominator
@custom:invariant forall bytes32 h :: ST(h).isCancelled ==> !ST(h).isValidated
@custom:reentrancy-guard GUARD

# ---------------------------------------------------------------- getters

== function Consideration.getCounter
@custom:ensures counter == _counters[offerer]

== function Consideration.getOrderStatus
@custom:ensures isValidated == ST(orderHash).isValidated
@custom:ensures isCancelled == ST(orderHash).isCancelled
@custom:ensures totalFilled == ST(orderHash).numerator
@custom:ensures totalSize == ST(orderHash).denominator

== function Consideration.getContractOffererNonce
@custom:ensures nonce == _contractNonces[contractOfferer]

# ---------------------------------------------------------------- counter

== function Consideration.incrementCounter
# Only the caller's counter moves, by less than 2^128 (mod 2^256), and the
# new value is returned.
@custom:ensures newCounter == _counters[msg.sender]
@custom:ensures (_counters[msg.sender] + 2**256 - old(_counters[msg.sender])) % 2**256 < 2**128
@custom:ensures forall address a :: a != msg.sender ==> _counters[a] == old(_counters[a])
@custom:ensures STATUS_KEPT
@custom:ensures NONCES_KEPT

# ---------------------------------------------------------------- cancel / validate

== function Consideration.cancel
# Only an order's offerer or zone may cancel it, never a contract order.
@custom:ensures forall uint256 i :: i < orders.length ==> uint8(orders[i].orderType) != 4 && (msg.sender == orders[i].offerer || msg.sender == orders[i].zone)
# Every status it touches ends cancelled and not validated; fills are kept.
@custom:ensures forall bytes32 h :: (ST(h).isCancelled == old(ST(h).isCancelled) && ST(h).isValidated == old(ST(h).isValidated)) || (ST(h).isCancelled && !ST(h).isValidated)
@custom:ensures forall bytes32 h :: old(ST(h).isCancelled) ==> ST(h).isCancelled
@custom:ensures FILLS_KEPT
@custom:ensures COUNTERS_KEPT
@custom:ensures NONCES_KEPT
@custom:ensures cancelled

== function Consideration.validate
# Validation only sets isValidated; it never cancels, uncancels or fills.
@custom:ensures forall bytes32 h :: ST(h).isCancelled == old(ST(h).isCancelled)
@custom:ensures forall bytes32 h :: old(ST(h).isValidated) ==> ST(h).isValidated
@custom:ensures FILLS_KEPT
@custom:ensures COUNTERS_KEPT
@custom:ensures NONCES_KEPT

# ---------------------------------------------------------------- fulfillment

== function Consideration.fulfillBasicOrder
FILL_RULES
@custom:ensures address(this).balance == old(address(this).balance)

== function Consideration.fulfillBasicOrder_efficient_6GL6yc
FILL_RULES
@custom:ensures address(this).balance == old(address(this).balance)

== function Consideration.fulfillOrder
FILL_RULES
RETAINS_NOTHING

== function Consideration.fulfillAdvancedOrder
FILL_RULES
RETAINS_NOTHING

== function Consideration.fulfillAvailableOrders
FILL_RULES
RETAINS_NOTHING

== function Consideration.fulfillAvailableAdvancedOrders
FILL_RULES
RETAINS_NOTHING

== function Consideration.matchOrders
FILL_RULES
RETAINS_NOTHING

== function Consideration.matchAdvancedOrders
FILL_RULES
RETAINS_NOTHING
