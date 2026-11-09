//@ codegen-matrix: standard
//@ run-call: inOrder => 1
//@ run-call: threeInOrder => 1
// Copies of storage arrays allocate in argument order, also when only an earlier one has to wait
// for a later operand that writes storage.
contract C {
    uint256[] first = [1, 2];
    uint256[] second = [3, 4];
    uint256[] third = [5, 6];

    function pickSecond() internal returns (uint256[] storage) {
        first[0] = 9;
        return second;
    }

    function ordered(uint256[] memory a, uint256[] memory b) internal pure returns (uint256 result) {
        assembly {
            result := lt(a, b)
        }
    }

    function orderedThree(uint256[] memory a, uint256[] memory b, uint256[] memory c)
        internal
        pure
        returns (uint256 result)
    {
        assembly {
            result := and(lt(a, b), lt(b, c))
        }
    }

    function pickThird() internal returns (uint256[] storage) {
        first[0] = 9;
        return third;
    }

    function inOrder() public returns (uint256) {
        return ordered(first, pickSecond());
    }

    function threeInOrder() public returns (uint256) {
        return orderedThree(first, second, pickThird());
    }
}
