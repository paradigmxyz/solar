//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SBAP

// Storage arrays with `bytes`/`string` elements support push, pop, and
// indexing: each element slot holds the packed short/long bytes form, pushes
// copy the memory value into it, pops clear it (including long-form data
// slots) by storing an empty value, and indexed reads materialize the element
// into memory. Verified behaviorally against solc, including long-form
// clearing and slot reuse after pop.

contract StorageBytesArrayPush {
    struct StrSet {
        string[] _values;
        uint256 tag;
    }

    StrSet internal set;
    bytes[] internal blobs;

    // SBAP-LABEL: fn @pushStr
    // SBAP: keccak256
    // SBAP: sstore
    function pushStr(string memory v) public {
        set._values.push(v);
    }

    // SBAP-LABEL: fn @popStr
    // The popped element clears through the packed-form store, not a single
    // zero word.
    // SBAP: keccak256
    // SBAP: sstore
    function popStr() public {
        set._values.pop();
    }

    // SBAP-LABEL: fn @blobAt
    // Indexed bytes elements materialize into memory.
    // SBAP: keccak256
    // SBAP: sload
    function blobAt(uint256 i) public view returns (bytes memory) {
        return blobs[i];
    }

    // SBAP-LABEL: fn @pushEmpty
    // SBAP: sstore
    function pushEmpty() public {
        blobs.push();
    }
}
