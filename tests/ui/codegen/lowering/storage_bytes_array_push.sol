//@compile-flags: -Zdump=mir
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
    // SBAP: 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
    // SBAP: sstore
    function pushStr(string memory v) public {
        set._values.push(v);
    }

    // SBAP-LABEL: fn @popStr
    // The popped element clears through the packed-form store, not a single
    // zero word.
    // SBAP: 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
    // SBAP: sstore
    function popStr() public {
        set._values.pop();
    }

    // SBAP-LABEL: fn @blobAt
    // Indexed bytes elements materialize into memory.
    // SBAP: sload
    // SBAP: 0x405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace
    function blobAt(uint256 i) public view returns (bytes memory) {
        return blobs[i];
    }

    // SBAP-LABEL: fn @pushEmpty
    // SBAP: sstore
    function pushEmpty() public {
        blobs.push();
    }
}
