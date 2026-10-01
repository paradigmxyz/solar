//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdataflow-optimizations
//@ run-call: transientReader => 1
//@ run-call: indirectOverwrite => 16
//@ run-call-fail: indirectCallback => Error("locked")
contract CallBarriers {
    uint256 unlocked = 1;
    uint256 value;

    function transientReader() external returns (uint256 result) {
        assembly { tstore(0, 1) }
        result = readTransient();
        assembly { tstore(0, 0) }
    }

    function readTransient() internal view returns (uint256 result) {
        assembly { result := tload(0) }
    }

    function indirectOverwrite() external returns (uint256) {
        value = 7;
        uint256 before = value;
        overwrite();
        return before + value;
    }

    function overwrite() internal {
        this.write();
    }

    function write() external { value = 9; }

    function indirectCallback() external {
        require(unlocked == 1, "locked");
        unlocked = 0;
        callback();
        unlocked = 1;
    }

    function callback() internal { this.indirectCallback(); }
}
