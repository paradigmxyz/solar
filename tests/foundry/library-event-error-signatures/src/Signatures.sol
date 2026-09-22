// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract C {
    function selectors() external pure returns (bool) {
        assert(L.E.selector == keccak256("E(address,uint8,(address,uint8,uint256))"));
        assert(L.F.selector == bytes4(keccak256("F(address,uint8,(address,uint8,uint256))")));
        assert(L.f.selector == bytes4(keccak256("f(C,L.Kind,L.S)")));
        return true;
    }

    function fail() external pure {
        revert L.F(C(address(1)), L.Kind.B, L.S(C(address(2)), L.Kind.A, 3));
    }

    function errorData() external returns (bool) {
        (bool ok, bytes memory data) = address(this).call(abi.encodeWithSignature("fail()"));
        assert(!ok);
        assert(keccak256(data) == keccak256(abi.encodeWithSelector(
            bytes4(keccak256("F(address,uint8,(address,uint8,uint256))")),
            address(1), uint8(1), L.S(C(address(2)), L.Kind.A, 3)
        )));
        return true;
    }

    function fire() external {
        emit L.E(C(address(1)), L.Kind.B, L.S(C(address(2)), L.Kind.A, 3));
    }
}

library L {
    enum Kind { A, B }
    struct S { C c; Kind k; uint256 n; }

    /// @notice Emitted values
    /// @dev Event details
    /// @param c Contract value
    /// @param k Enum value
    /// @param s Struct value
    event E(C c, Kind k, S s);

    /// @notice Rejected values
    /// @dev Error details
    /// @param c Contract value
    /// @param k Enum value
    /// @param s Struct value
    error F(C c, Kind k, S s);

    /// @notice Library function
    /// @dev Function details
    function f(C c, Kind k, S memory s) external pure returns (uint256) {
        return uint256(uint160(address(c))) + uint256(k) + s.n;
    }
}
