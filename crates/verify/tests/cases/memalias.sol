pragma solidity ^0.8.20;
/// Memory arrays and structs passed to internal functions: the callee writes the caller's object,
/// but the encoder binds a copy, so such calls are refused until the memory model shares them.
contract MemAlias {
    struct P { uint256 a; }

    function set(uint256[] memory a) internal pure { a[0] = 1; }
    function setP(P memory p) internal pure { p.a = 1; }
    function inc(uint256[] memory a) internal pure { a[0]++; }
    function fwd(uint256[] memory a) internal pure { set(a); }
    function first(uint256[] memory a) internal pure returns (uint256) { return a.length > 0 ? a[0] : 0; }

    /// The callee's write is visible to the caller: `xs[0] == 1` afterwards.
    // expect: memory array or struct passed to internal function `set`
    /// @custom:ensures result == 0
    function bad_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        set(xs);
        return xs[0];
    }
    // expect: memory array or struct passed to internal function `setP`
    /// @custom:ensures result == 0
    function bad_alias_struct() public pure returns (uint256) {
        P memory p = P(0);
        setP(p);
        return p.a;
    }
    // expect: memory array or struct passed to internal function `inc`
    /// @custom:ensures result == 0
    function bad_alias_inc(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        inc(xs);
        return xs[0];
    }
    // expect: memory array or struct passed to internal function `fwd`
    /// @custom:ensures result == 0
    function bad_alias_forward(uint256[] memory xs) public pure returns (uint256) {
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

    // expect: memory array or struct `b` initialized from another one
    /// @custom:ensures result == 0
    function bad_local_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        uint256[] memory b = xs;
        b[0] = 1;
        return xs[0];
    }

    // expect: memory array or struct assigned from another one
    /// @custom:ensures result == 0
    function bad_assign_alias(uint256[] memory xs, uint256[] memory ys) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        ys = xs;
        ys[0] = 1;
        return xs[0];
    }

    function id(uint256[] memory a) internal pure returns (uint256[] memory) {
        return a;
    }

    // expect: memory array or struct `b` initialized from another one
    /// @custom:ensures result == 0
    function bad_ret_alias(uint256[] memory xs) public pure returns (uint256) {
        require(xs.length > 0);
        xs[0] = 0;
        uint256[] memory b = id(xs);
        b[0] = 1;
        return xs[0];
    }

    /// @custom:ensures result == 0
    function ok_fresh(uint256 n) public pure returns (uint256) {
        uint256[] memory b = new uint256[](n + 1);
        b[0] = 0;
        return b[0];
    }
}
