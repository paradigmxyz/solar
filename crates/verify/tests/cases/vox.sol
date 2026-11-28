pragma solidity ^0.8.20;
/// Lessons from Vox (julesjacobs/oxcaml, branch vox), each as an accepted program, a nearby
/// rejected one with its counterexample pinned (// expect:), and a positive control.

// 1. Checked claims: @custom:check P runs as a require (--instrument) and is then a fact.
contract Checked {
    uint256 public total;
    /// @custom:check amount <= 1000
    /// @custom:ensures total == old(total) + amount
    function ok_checked_pre(uint256 amount) external {
        require(total <= 1000000);
        total += amount;
    }
    /// The caller of a checked helper is never blamed: a bad argument reverts in the check.
    function ok_caller_not_blamed(uint256 x) external pure returns (uint256) {
        return half(x);
    }
    /// @custom:check x % 2 == 0
    function half(uint256 x) internal pure returns (uint256) { return x / 2; }

    /// A check is a fact for what follows, but not for what it does not say.
    // expect: postcondition not satisfied
    // expect: x = 1000
    /// @custom:ensures result < 1000
    function bad_check_too_weak(uint256 x) external pure returns (uint256) {
        /// @custom:check x <= 1000
        uint256 y = x;
        return y;
    }
    /// Positive control: the same function with a check strong enough.
    /// @custom:ensures result < 1000
    function ok_check_strong(uint256 x) external pure returns (uint256) {
        /// @custom:check x < 1000
        uint256 y = x;
        return y;
    }
    /// A check on a liveness path is a revert point: succeeds_if blames it.
    // expect: x = 7
    /// @custom:succeeds-if true
    function bad_check_reverts(uint256 x) external pure returns (uint256) {
        /// @custom:check x != 7
        uint256 y = x;
        return y;
    }
    /// Proved checks are reported as removable (a note in the output).
    // expect: is proved here
    function ok_check_redundant(uint256 x) external pure returns (uint256) {
        if (x > 5) {
            /// @custom:check x > 3
            return x;
        }
        return 0;
    }
}

// 2. Ghost lemmas (@custom:use, erased) and opaque definitions (@custom:opaque + @custom:reveal).
library Lemmas {
    /// @custom:requires a <= b
    /// @custom:ensures a * c <= b * c
    function mulMono(uint256 a, uint256 b, uint256 c) internal pure {}

    /// @custom:opaque
    function within(uint256 x, uint256 lo, uint256 hi) internal pure returns (bool) {
        return lo <= x && x < hi;
    }

    /// @custom:requires x <= y
    /// @custom:ensures result <= y * k
    function ok_use_lemma(uint256 x, uint256 y, uint256 k) internal pure returns (uint256) {
        /// @custom:use mulMono(x, y, k)
        uint256 r = x * k;
        return r;
    }
    /// The lemma's precondition is checked at the use: here nothing gives x <= y.
    // expect: precondition of lemma `mulMono` not satisfied
    /// @custom:ensures result <= y * k
    function bad_use_lemma_pre(uint256 x, uint256 y, uint256 k) internal pure returns (uint256) {
        /// @custom:use mulMono(x, y, k)
        uint256 r = x * k;
        return r;
    }
    /// Opaque: the body is not visible without a reveal.
    // expect: postcondition not satisfied
    /// @custom:requires within(x, 0, 10)
    /// @custom:ensures result < 10
    function bad_opaque_hidden(uint256 x) internal pure returns (uint256) { return x; }
    /// @custom:requires within(x, 0, 10)
    /// @custom:ensures result < 10
    function ok_opaque_revealed(uint256 x) internal pure returns (uint256) {
        /// @custom:reveal within(x, 0, 10)
        uint256 r = x;
        return r;
    }
}

// A lemma may not be used to prove itself (reported as unsupported).
library Cycle {
    // expect: cyclic proof is not allowed
    /// @custom:ensures false
    function bad_circular() internal pure {
        /// @custom:use bad_circular()
        uint256 z = 0;
        z;
    }
}

// 3. Interfaces as exported contracts: callers are checked against them, not bodies.
interface IOracle {
    /// @custom:requires maxAge > 0
    /// @custom:ensures price > 0 && age <= maxAge
    function read(uint256 maxAge) external view returns (uint256 price, uint256 age);
}
contract Client {
    IOracle oracle;
    /// @custom:ensures result > 0
    function ok_iface(uint256 maxAge) external view returns (uint256) {
        require(maxAge > 0);
        (uint256 p, ) = oracle.read(maxAge);
        return p;
    }
    // expect: precondition of external `IOracle.read` not satisfied
    // expect: maxAge = 0
    function bad_iface_pre(uint256 maxAge) external view returns (uint256) {
        (uint256 p, ) = oracle.read(maxAge);
        return p;
    }
    /// Positive control: the ensures give price > 0, not price > 1.
    // expect: postcondition not satisfied
    /// @custom:ensures result > 1
    function bad_iface_weak(uint256 maxAge) external view returns (uint256) {
        require(maxAge > 0);
        (uint256 p, ) = oracle.read(maxAge);
        return p;
    }
}

// 4. Struct invariants: a later field's refinement over earlier fields (Vox dependent fields).
/// @custom:invariant lo <= hi
struct Range { uint256 lo; uint256 hi; }
/// @custom:invariant x != 0
struct NonZero { uint256 x; }
contract Ranges {
    mapping(uint256 => Range) ranges;
    Range cur;
    function ok_construct(uint256 a) external {
        require(a < 1000);
        ranges[a] = Range(a, a + 1);
    }
    // expect: invariant of struct `Range` not established at construction
    function bad_construct(uint256 a, uint256 b) external {
        ranges[a] = Range(a, b);
    }
    /// projection from storage: every write was checked
    /// @custom:ensures result >= ranges[k].lo
    function ok_projection(uint256 k) external view returns (uint256) {
        return ranges[k].hi;
    }
    // expect: invariant of struct `Range` may not hold for the value stored
    function bad_field_write(uint256 k, uint256 v) external {
        ranges[k].hi = v;
    }
    function ok_field_write(uint256 k, uint256 v) external {
        require(v >= ranges[k].lo);
        ranges[k].hi = v;
    }
    /// calldata is not trusted: storing it unchecked fails
    // expect: invariant of struct `Range` may not hold for the value stored
    function bad_store_calldata(Range calldata r) external {
        cur = r;
    }
    function ok_store_validated(Range calldata r) external {
        require(r.lo <= r.hi);
        cur = r;
    }
}
contract Zero {
    mapping(uint256 => NonZero) m;
    // expect: does not hold for its zero value
    /// @custom:ensures result != 0
    function bad_zero(uint256 k) external view returns (uint256) {
        return m[k].x;
    }
}

// 5. Implementations inherit an interface's exported contract (Liskov): what callers trust is proved.
interface ICounter {
    /// @custom:requires by > 0
    /// @custom:ensures result > 0
    function bump(uint256 by) external returns (uint256);
}
contract GoodCounter is ICounter {
    uint256 n;
    function bump(uint256 by) external returns (uint256) {
        require(n < 1e30 && by < 1e30);
        n += by;
        return n;
    }
}
contract BadCounter is ICounter {
    uint256 n;
    // expect: postcondition not satisfied
    // expect: by =
    function bump(uint256 by) external returns (uint256) {
        return n * by;
    }
}

// 6. Struct invariants hold for pushes onto storage arrays, and interface specs see msg.sender as the caller.
contract RangeList {
    Range[] list;
    function ok_push(uint256 a) external {
        require(a < 1000);
        list.push(Range(a, a));
    }
    // expect: may not hold for the value stored
    function bad_push(Range calldata r) external {
        list.push(r);
    }
}
interface IPull {
    /// @custom:ensures from == msg.sender
    function pull(address from) external returns (bool);
}
contract Puller {
    IPull p;
    /// @custom:ensures result == address(this)
    function ok_sender(address a) external returns (address) {
        p.pull(a);
        return a;
    }
}
