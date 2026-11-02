//@compile-flags: -Zdump=mir
//@filecheck: --check-prefix=ADDA

// `abi.decode` into dynamic arrays of elementary types: the head offset and
// array bounds validate against the encoded region, word elements bulk-copy
// with a cleanliness sweep where the element type requires one, and
// `bytes`/`string` elements decode individually against the array's data
// region. Verified behaviorally against solc, including reverts on dirty
// and truncated payloads.

contract AbiDecodeDynamicArrays {
    // ADDA-LABEL: fn @words
    // Bulk copy, no per-element validation for full-word elements.
    // ADDA: {{(^|[^a-z_])}}mcopy
    function words(bytes memory b) public pure returns (uint256[] memory) {
        return abi.decode(b, (uint256[]));
    }

    // ADDA-LABEL: fn @bools
    // A validation loop on decode (dirty words revert), then the return
    // encoder canonicalizes each element in place instead of bulk-copying.
    // ADDA: jumpi
    // ADDA: = ne {{v[0-9]+}}, 0
    // ADDA-NOT: {{(^|[^a-z_])}}mcopy {{[^cr]}}
    // ADDA: returndata
    // ADDA: [[VALID:v[0-9]+]] = lt {{v[0-9]+}}, 2
    // ADDA-NEXT: jumpi [[VALID]], {{bb[0-9]+}}, [[INVALID:bb[0-9]+]]
    // ADDA: [[INVALID]] [cold]:
    // ADDA-NEXT: revert 0, 0
    function bools(bytes memory b) public pure returns (bool[] memory) {
        return abi.decode(b, (bool[]));
    }

    // ADDA-LABEL: fn @strs
    // Element-wise decode: per-element offsets resolve against the array's
    // own data region.
    // ADDA: {{(^|[^a-z_])}}mcopy
    function strs(bytes memory b) public pure returns (string[] memory) {
        return abi.decode(b, (string[]));
    }

    // ADDA-LABEL: fn @mixed
    // ADDA: {{(^|[^a-z_])}}mcopy
    function mixed(bytes memory b) public pure returns (uint256, uint256[] memory, string memory) {
        return abi.decode(b, (uint256, uint256[], string));
    }
}
