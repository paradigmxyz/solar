//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@ normalize-stdout-test: "(?s).+" -> ""
//@[gas] filecheck: --check-prefix=GAS
//@[size] filecheck: --check-prefix=SIZE
//@ run-call: words 3 => [0, 1, 2]
//@ run-call: text 2 => 0x6162

// Size builds store the free-memory pointer once, before dispatch, when two
// entries allocate; gas builds store it in each entry that needs it.
contract C {
    // GAS-LABEL: @module C_runtime
    // GAS-NEXT: bb0:
    // GAS-NOT: push 64
    // GAS: callvalue
    // SIZE-LABEL: @module C_runtime
    // SIZE-NEXT: bb0:
    // SIZE-NEXT: push [[START:[0-9]+]]
    // SIZE-NEXT: push 64
    // SIZE-NEXT: mstore
    // SIZE-NOT: push [[START]]{{$}}
    function words(uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        for (uint256 i; i < n; ++i) out[i] = i;
    }

    function text(uint256 n) external pure returns (bytes memory out) {
        out = new bytes(n);
        for (uint256 i; i < n; ++i) out[i] = bytes1(uint8(97 + i));
    }
}
