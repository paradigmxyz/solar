//@ codegen-matrix: standard
//@ run-call: C::direct => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: C::viaReturn => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: C::viaLocal => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf

// `ecrecover` reads `v` as a whole word, so an assembly-assigned `uint8` must be
// cleaned to its declared width first, as solc does. Each `v` below carries dirty
// high bits above the valid recovery id 27.

contract C {
    bytes32 constant HASH = 0x2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824;
    bytes32 constant R = 0x0f2fff8620d8ffe97040f8cf72ae476ef8ff4412373929c0324ce8428d3352e7;
    bytes32 constant S = 0x1845ae4903027667005846f8f0be3e5ed2db5c3826ba83a6e542e080792f9a9d;

    function dirtyV() internal pure returns (uint8 v) {
        assembly {
            v := 0xff1b
        }
    }

    function direct() external pure returns (uint256) {
        uint8 v;
        assembly {
            v := 0xff1b
        }
        return uint160(ecrecover(HASH, v, R, S));
    }

    function viaReturn() external pure returns (uint256) {
        return uint160(ecrecover(HASH, dirtyV(), R, S));
    }

    function viaLocal() external pure returns (uint256) {
        uint8 v = dirtyV();
        uint8 w = v;
        return uint160(ecrecover(HASH, w, R, S));
    }
}
