//@ run-call: values => 7, 5, 4, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45
//@ run-call: references => 0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45, 0x6162630000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000001234, true, 0x971217b7

// Assembly reads value-type constants whose values are not number literals. solc rejects these
// reads with error 7615; see TYPECK-012 in docs/SOLC_DIVERGENCE.md.
contract AssemblyConstantExpressions {
    uint256 constant three = 3;
    uint256 constant converted = uint256(7);
    uint256 constant conditional = true ? 5 : 6;
    uint256 constant sum = three + 1;
    uint256 constant inverted = ~uint256(0);
    bytes32 constant hash = keccak256("abc");
    bytes32 constant hashRef = hash;
    bytes32 constant parenthesized = ("abc");
    address constant account = address(0x1234);
    bool constant comparison = 1 < 2;
    bytes4 constant selector = this.values.selector;

    function values()
        external
        pure
        returns (uint256 a, uint256 b, uint256 c, uint256 d, bytes32 e)
    {
        assembly {
            a := converted
            b := conditional
            c := sum
            d := inverted
            e := hash
        }
    }

    function references()
        external
        pure
        returns (bytes32 a, bytes32 b, address c, bool d, bytes4 e)
    {
        assembly {
            a := hashRef
            b := parenthesized
            c := account
            d := comparison
            e := selector
        }
    }
}
