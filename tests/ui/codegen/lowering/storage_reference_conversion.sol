//@compile-flags: --emit=bin

interface I {
    struct S {
        uint256 x;
    }
}

contract StorageReferenceConversion {
    I.S[] records;
    uint256[] nums;

    // The conversion result still refers to storage, so writes through it must reach the state
    // variables rather than a memory copy.
    function structField() external {
        records.push();
        I.S[](records)[0].x = 5; //~ ERROR: codegen rewrite does not support this storage reference conversion yet
    }

    function element() external {
        nums.push();
        uint256[](nums)[0] = 7; //~ ERROR: codegen rewrite does not support this storage reference conversion yet
    }
}
