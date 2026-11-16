// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Free functions and internal function pointers: every ok_* must verify, every bad_* must fail.

function addOne(uint256 x) pure returns (uint256) {
    return x + 1;
}

function double(uint256 x) pure returns (uint256) {
    return x * 2;
}

library L {
    function triple(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }
}

contract FnPtr {
    uint256 internal total;
    function(uint256) internal pure returns (uint256) internal stored;

    function inc(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    function dec(uint256 x) internal pure returns (uint256) {
        return x - 1;
    }

    function narrow(uint8 x) internal pure returns (uint256) {
        return x;
    }

    function bump(uint256 by) internal {
        total += by;
    }

    function applyTo(function(uint256) internal pure returns (uint256) f, uint256 x)
        internal
        pure
        returns (uint256)
    {
        return f(x);
    }

    /// The cast Seaport uses to reuse code: the pointer is copied in assembly.
    function same(function(uint256) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint256) internal pure returns (uint256) outFn)
    {
        assembly {
            outFn := inFn
        }
    }

    function shifted(function(uint256) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint256) internal pure returns (uint256) outFn)
    {
        assembly {
            outFn := add(inFn, 1)
        }
    }

    function toNarrow(function(uint256) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(uint8) internal pure returns (uint256) outFn)
    {
        assembly {
            outFn := inFn
        }
    }

    // ---------------------------------------------------------------- free functions
    /// @custom:ensures result == x + 1
    function ok_free(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        result = addOne(x);
    }

    /// @custom:ensures result == x
    function bad_free(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        result = addOne(x);
    }

    /// A free function's checked arithmetic reverts like any other.
    /// @custom:ensures result == 2 * x
    function ok_free_checked(uint256 x) public pure returns (uint256 result) {
        result = double(x);
    }

    // ---------------------------------------------------------------- pointers
    /// @custom:ensures result == x + 1
    function ok_ptr_local(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        function(uint256) internal pure returns (uint256) f = inc;
        result = f(x);
    }

    /// @custom:ensures up ==> result == x + 1
    /// @custom:ensures !up ==> result == x - 1
    function ok_ptr_choice(uint256 x, bool up) public pure returns (uint256 result) {
        require(x > 0 && x < 100);
        function(uint256) internal pure returns (uint256) f = up ? inc : dec;
        result = f(x);
    }

    /// @custom:ensures result == x + 1
    function bad_ptr_choice(uint256 x, bool up) public pure returns (uint256 result) {
        require(x > 0 && x < 100);
        function(uint256) internal pure returns (uint256) f = up ? inc : dec;
        result = f(x);
    }

    /// @custom:ensures result == x + 1
    function ok_ptr_param(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        result = applyTo(inc, x);
    }

    /// @custom:ensures result == x - 1
    function bad_ptr_param(uint256 x) public pure returns (uint256 result) {
        require(x > 0 && x < 100);
        result = applyTo(inc, x);
    }

    /// @custom:ensures result == 3 * x + 2 * x
    function ok_ptr_library_and_free(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        function(uint256) internal pure returns (uint256) f = L.triple;
        function(uint256) internal pure returns (uint256) g = double;
        result = f(x) + g(x);
    }

    /// The call may write what its target writes.
    /// @custom:ensures total == old(total) + by
    function ok_ptr_state(uint256 by) public {
        require(by < 100 && total < 100);
        function(uint256) internal g = bump;
        g(by);
    }

    /// @custom:ensures total == old(total)
    function bad_ptr_state(uint256 by) public {
        require(by < 100 && total < 100);
        function(uint256) internal g = bump;
        g(by);
    }

    /// @custom:ensures result == x + 1
    function ok_ptr_assembly_copy(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        result = same(inc)(x);
    }

    /// @custom:ensures result == x
    function bad_ptr_assembly_copy(uint256 x) public pure returns (uint256 result) {
        require(x < 100);
        result = same(inc)(x);
    }

    /// An unset pointer panics.
    function bad_strict_only_unset(uint256 x) public pure returns (uint256) {
        function(uint256) internal pure returns (uint256) f;
        return f(x);
    }

    // expect: may hold no internal function of its type
    function bad_ptr_arithmetic(uint256 x) public pure returns (uint256) {
        return shifted(inc)(x);
    }

    // expect: casts that reinterpret a value are not modelled
    function bad_ptr_reinterpret(uint8 x) public pure returns (uint256) {
        return toNarrow(inc)(x);
    }

    // expect: may hold no internal function of its type
    function bad_ptr_storage(uint256 x) public view returns (uint256) {
        return stored(x);
    }
}
