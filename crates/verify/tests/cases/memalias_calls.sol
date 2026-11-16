pragma solidity ^0.8.20;

library Ids {
    function same(uint256[] memory a) internal pure returns (uint256[] memory) {
        return a;
    }
}

/// Memory arrays and structs passed to internal functions: the callee writes the caller's object,
/// and the caller's name for it takes the callee's final value. Where the argument is not a name
/// the write can go back to, or two names may share one object that either writes, the function
/// is refused. Every ok_* must verify, every bad_* must fail.
contract MemAlias {
    using Ids for uint256[];

    struct P { uint256 a; }
    struct Q { uint256[] xs; }

    function set(uint256[] memory a) internal pure { a[0] = 1; }
    function setP(P memory p) internal pure { p.a = 1; }
    function inc(uint256[] memory a) internal pure { a[0]++; }
    function fwd(uint256[] memory a) internal pure { set(a); }
    function first(uint256[] memory a) internal pure returns (uint256) { return a.length > 0 ? a[0] : 0; }
    function id(uint256[] memory a) internal pure returns (uint256[] memory) { return a; }
    function pair(uint256[] memory a) internal pure returns (uint256[] memory, uint256) { return (a, 0); }

    /// The callee's write is visible to the caller: `xs[0] == 1` afterwards.
    // expect: postcondition not satisfied
    /// @custom:ensures result == 0
    function bad_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        set(xs);
        return xs[0];
    }
    /// @custom:ensures result == 1
    function ok_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        set(xs);
        return xs[0];
    }
    // expect: postcondition not satisfied
    /// @custom:ensures result == 0
    function bad_alias_struct() public pure returns (uint256) {
        P memory p = P(0);
        setP(p);
        return p.a;
    }
    /// @custom:ensures result == 1
    function ok_alias_struct() public pure returns (uint256) {
        P memory p = P(0);
        setP(p);
        return p.a;
    }
    // expect: postcondition not satisfied
    /// @custom:ensures result == 0
    function bad_alias_inc(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        inc(xs);
        return xs[0];
    }
    // expect: postcondition not satisfied
    /// @custom:ensures result == 0
    function bad_alias_forward(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        fwd(xs);
        return xs[0];
    }
    /// @custom:ensures result == 1
    function ok_alias_forward(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        fwd(xs);
        return xs[0];
    }
    /// A callee that only reads its array is still verified.
    /// @custom:ensures result == 7
    function ok_readonly(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 7;
        return first(xs);
    }

    // A written argument the write cannot go back to is refused.
    // expect: may alias a caller object the callee writes
    function bad_cond_arg(uint256[] memory xs, uint256[] memory ys, bool c) public pure {
        require(xs.length > 0 && ys.length > 0);
        set(c ? xs : ys);
    }

    // expect: aliased memory is not modelled
    /// @custom:ensures result == 0
    function bad_local_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        uint256[] memory b = xs;
        b[0] = 1;
        return xs[0];
    }

    // expect: aliased memory is not modelled
    /// @custom:ensures result == 0
    function bad_assign_alias(uint256[] memory xs, uint256[] memory ys) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        ys = xs;
        ys[0] = 1;
        return xs[0];
    }

    // expect: aliased memory is not modelled
    /// @custom:ensures result == 0
    function bad_ret_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        uint256[] memory b = id(xs);
        b[0] = 1;
        return xs[0];
    }

    // The receiver of a library call may come back too.
    // expect: may share its memory with another reference
    /// @custom:ensures result == 0
    function bad_receiver_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        uint256[] memory b = xs.same();
        b[0] = 1;
        return xs[0];
    }

    // expect: may share its memory with another reference
    /// @custom:ensures result == 0
    function bad_tuple_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        (uint256[] memory b, ) = pair(xs);
        b[0] = 1;
        return xs[0];
    }

    // A field bound to an existing array shares it.
    // expect: may share its memory with another reference
    /// @custom:ensures result == 0
    function bad_field_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        Q memory q = Q(new uint256[](1));
        q.xs = xs;
        q.xs[0] = 1;
        return xs[0];
    }

    /// @custom:ensures result == 0
    function ok_fresh(uint256 n) public pure returns (uint256) {
        uint256[] memory b = new uint256[](n + 1);
        b[0] = 0;
        return b[0];
    }
}
