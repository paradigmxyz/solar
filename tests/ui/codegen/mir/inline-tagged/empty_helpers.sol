//@ compile-flags: -Ogas --emit=bin
//@ normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"

// Each tagged helper calls the one before it twice, and the first has an empty body. Cloning
// every call would double the blocks of `run` with each helper even though no instruction is
// cloned, so the budget of tagged clones counts blocks as well as instructions.
contract EmptyTaggedHelpers {
    /// @custom:solar-inline always
    function h0() internal pure {}

    /// @custom:solar-inline always
    function h1() internal pure {
        h0();
        h0();
    }

    /// @custom:solar-inline always
    function h2() internal pure {
        h1();
        h1();
    }

    /// @custom:solar-inline always
    function h3() internal pure {
        h2();
        h2();
    }

    /// @custom:solar-inline always
    function h4() internal pure {
        h3();
        h3();
    }

    /// @custom:solar-inline always
    function h5() internal pure {
        h4();
        h4();
    }

    /// @custom:solar-inline always
    function h6() internal pure {
        h5();
        h5();
    }

    /// @custom:solar-inline always
    function h7() internal pure {
        h6();
        h6();
    }

    /// @custom:solar-inline always
    function h8() internal pure {
        h7();
        h7();
    }

    /// @custom:solar-inline always
    function h9() internal pure {
        h8();
        h8();
    }

    /// @custom:solar-inline always
    function h10() internal pure {
        h9();
        h9();
    }

    /// @custom:solar-inline always
    function h11() internal pure {
        h10();
        h10();
    }

    /// @custom:solar-inline always
    function h12() internal pure {
        h11();
        h11();
    }

    /// @custom:solar-inline always
    function h13() internal pure {
        h12();
        h12();
    }

    /// @custom:solar-inline always
    function h14() internal pure {
        h13();
        h13();
    }

    /// @custom:solar-inline always
    function h15() internal pure {
        h14();
        h14();
    }

    /// @custom:solar-inline always
    function h16() internal pure {
        h15();
        h15();
    }

    /// @custom:solar-inline always
    function h17() internal pure {
        h16();
        h16();
    }

    /// @custom:solar-inline always
    function h18() internal pure {
        h17();
        h17();
    }

    /// @custom:solar-inline always
    function h19() internal pure {
        h18();
        h18();
    }

    /// @custom:solar-inline always
    function h20() internal pure {
        h19();
        h19();
    }

    /// @custom:solar-inline always
    function h21() internal pure {
        h20();
        h20();
    }

    /// @custom:solar-inline always
    function h22() internal pure {
        h21();
        h21();
    }

    /// @custom:solar-inline always
    function h23() internal pure {
        h22();
        h22();
    }

    /// @custom:solar-inline always
    function h24() internal pure {
        h23();
        h23();
    }

    /// @custom:solar-inline always
    function h25() internal pure {
        h24();
        h24();
    }

    function run() external pure returns (uint256) {
        h25();
        return 7;
    }
}
