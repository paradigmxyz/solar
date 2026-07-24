//@compile-flags: -Zcodegen -O none -Zdump=evm-ir-runtime
//@filecheck: --check-prefix=NONE --enable-var-scope

contract ColdCallFallthroughNone {
    // NONE-LABEL: @module runtime
    // NONE: push 128
    // NONE-NEXT: mstore
    // NONE-NEXT: push 0{{$}}
    // NONE-NEXT: push 4{{$}}
    // NONE-NEXT: calldataload
    // NONE-NEXT: eq
    // NONE-NEXT: iszero
    // NONE-NEXT: push [[HOT:bb[0-9]+]]
    // NONE-NEXT: jumpi
    // NONE-NEXT: jump [[COLD:bb[0-9]+]]
    // NONE: [[COLD]]:
    // NONE: jump
    // NONE: [[HOT]]:
    // NONE: return
    function nonzero(uint256 value) external pure returns (uint256) {
        if (value == 0) abort(value);
        return value;
    }

    function abort(uint256 value) internal pure {
        if (value == 0) revert("zero");
        if (value == 1) revert("one");
        if (value == 2) revert("two");
        revert("other");
    }
}
