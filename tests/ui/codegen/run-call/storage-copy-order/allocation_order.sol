//@ codegen-matrix: standard
//@ run-call: inOrder => 1
//@ run-call: threeInOrder => 1
//@ run-call: calldataInOrder [1, 2] => 1
//@ run-call: literalInOrder => 1
//@ run-call: tupleDeclarationInOrder => 1
//@ run-call: returnTupleInOrder => 1
// Copies of storage arrays allocate in argument order, also when only an earlier one has to wait
// for a later operand that writes storage; a calldata array or a string literal between them is allocated in order too.
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

    function orderedCalldata(uint256[] memory a, uint256[] memory b, uint256[] memory c)
        internal
        pure
        returns (uint256 result)
    {
        assembly {
            result := and(lt(a, b), lt(b, c))
        }
    }

    function orderedLiteral(uint256[] memory a, string memory b, uint256[] memory c)
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

    function calldataInOrder(uint256[] calldata copied) public returns (uint256) {
        return orderedCalldata(first, copied, pickThird());
    }

    function literalInOrder() public returns (uint256) {
        return orderedLiteral(first, "abc", pickThird());
    }

    function tupleDeclarationInOrder() public returns (uint256) {
        (uint256[] memory a, uint256[] memory b, uint256[] memory c) =
            (first, second, pickThird());
        return orderedThree(a, b, c);
    }

    function returnTuple() internal returns (uint256[] memory, uint256[] memory, uint256[] memory) {
        return (first, second, pickThird());
    }

    function returnTupleInOrder() public returns (uint256) {
        (uint256[] memory a, uint256[] memory b, uint256[] memory c) = returnTuple();
        return orderedThree(a, b, c);
    }
}
