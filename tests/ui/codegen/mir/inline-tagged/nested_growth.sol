//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: run 0 => 77971143
//@ run-call: run 5 => 525687478
//@ run-call: run 57896044618658097711785492504343953926634992332820282019728792003956564819975 => 50504915241340554377552066422397662542973731256734524276785307364228376425831

// Each tagged helper calls the one before it twice, so cloning every tagged call would put 2^16
// bodies into `run`, and compile time would double with each helper. Tagged clones stop at a
// budget the whole module shares, and the calls left past it are weighed like any other: once CSE
// has merged the two clones in each body, which compute the same value, the rest inline into a
// chain of 16 steps.
contract TaggedNesting {
    /// @custom:solar-inline
    function h0(uint256 x) internal pure returns (uint256) {
        unchecked {
            return x + 1;
        }
    }

    /// @custom:solar-inline
    function h1(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h0(x) * 3 + (h0(x) >> 2);
        }
    }

    /// @custom:solar-inline
    function h2(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h1(x) * 3 + (h1(x) >> 3);
        }
    }

    /// @custom:solar-inline
    function h3(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h2(x) * 3 + (h2(x) >> 4);
        }
    }

    /// @custom:solar-inline
    function h4(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h3(x) * 3 + (h3(x) >> 5);
        }
    }

    /// @custom:solar-inline
    function h5(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h4(x) * 3 + (h4(x) >> 6);
        }
    }

    /// @custom:solar-inline
    function h6(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h5(x) * 3 + (h5(x) >> 7);
        }
    }

    /// @custom:solar-inline
    function h7(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h6(x) * 3 + (h6(x) >> 1);
        }
    }

    /// @custom:solar-inline
    function h8(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h7(x) * 3 + (h7(x) >> 2);
        }
    }

    /// @custom:solar-inline
    function h9(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h8(x) * 3 + (h8(x) >> 3);
        }
    }

    /// @custom:solar-inline
    function h10(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h9(x) * 3 + (h9(x) >> 4);
        }
    }

    /// @custom:solar-inline
    function h11(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h10(x) * 3 + (h10(x) >> 5);
        }
    }

    /// @custom:solar-inline
    function h12(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h11(x) * 3 + (h11(x) >> 6);
        }
    }

    /// @custom:solar-inline
    function h13(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h12(x) * 3 + (h12(x) >> 7);
        }
    }

    /// @custom:solar-inline
    function h14(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h13(x) * 3 + (h13(x) >> 1);
        }
    }

    /// @custom:solar-inline
    function h15(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h14(x) * 3 + (h14(x) >> 2);
        }
    }

    /// @custom:solar-inline
    function h16(uint256 x) internal pure returns (uint256) {
        unchecked {
            return h15(x) * 3 + (h15(x) >> 3);
        }
    }

    // CHECK-LABEL: fn @run(
    // CHECK-NOT: icall
    // CHECK: returndata
    function run(uint256 x) external pure returns (uint256) {
        return h16(x);
    }
}
