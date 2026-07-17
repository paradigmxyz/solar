// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface PairSource {
    function pair() external returns (uint256, uint256);
}

contract FixedPairSource is PairSource {
    function pair() external pure returns (uint256, uint256) {
        return (11, 22);
    }
}

contract TupleTernary {
    function choose(PairSource source, bool condition)
        external
        returns (uint256, uint256)
    {
        return condition ? source.pair() : (3, 4);
    }
}

contract TupleTernaryTest {
    function testTupleTernaryTrueArm() public {
        FixedPairSource source = new FixedPairSource();
        TupleTernary chooser = new TupleTernary();
        (uint256 a, uint256 b) = chooser.choose(source, true);

        assert(a == 11);
        assert(b == 22);
    }

    function testTupleTernaryFalseArm() public {
        FixedPairSource source = new FixedPairSource();
        TupleTernary chooser = new TupleTernary();
        (uint256 a, uint256 b) = chooser.choose(source, false);

        assert(a == 3);
        assert(b == 4);
    }
}
