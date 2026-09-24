//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

// A brace inside call arguments may start named arguments, including an empty list.
contract C {
    struct S { uint256 value; }

    function named(uint256 amount) internal pure returns (uint256) {
        return amount;
    }

    function empty() internal pure {}

    function externalCall(uint256 amount) external payable returns (uint256) {
        return amount;
    }

    function use(S memory s) public returns (uint256 result) {
        result = named(
            {amount: 1}
        );
        empty({});
        empty(
            {
            }
        );
        result += s.
            value;
        result += this.externalCall
            {gas: 100000, value: 0}
            ({amount: result});
        {
            {}
            if (result > 0) {
                for (uint256 i; i < 1; i++) {
                    while (result > 1) {
                        unchecked { result--; }
                    }
                }
            }
        }
    }
}
