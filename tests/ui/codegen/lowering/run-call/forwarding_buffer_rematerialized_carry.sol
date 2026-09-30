//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call-fail: 0x12345678 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012

// Seventeen values ride the stack across a call to a helper that may copy over low memory.
// Afterwards, unwinding pops the calldata loads above the deepest one instead of storing them,
// since they rematerialize. Empty storage makes the call divide by zero; only compiling it
// matters here.
// https://github.com/paradigmxyz/solar/issues/1625

contract C {
    function h1(uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12) internal pure returns (uint256, uint256, uint256, uint256, uint256) {
        unchecked {
            if ((((((uint256(1) == 0 ? uint256(1) : a11) & 1) == 0 ? (a9 ^ uint256(0)) : a5) + uint256(1)) & 1) == 0) {
                assembly { calldatacopy(128, 0, calldatasize()) }
            }
        }
    }
    fallback() external {
        unchecked {
            uint256 v36; assembly { v36 := calldataload(200) }
            uint256 v37; assembly { v37 := sload(7) }
            uint256 v38; assembly { v38 := calldataload(36) }
            uint256 v39; assembly { v39 := calldataload(300) }
            uint256 v40; assembly { v40 := calldataload(132) }
            uint256 v42; assembly { v42 := calldataload(132) }
            uint256 v43; assembly { v43 := sload(10) }
            uint256 v45; assembly { v45 := calldataload(32) }
            uint256 v47; assembly { v47 := sload(0) }
            uint256 v49; assembly { v49 := calldataload(132) }
            uint256 v50; assembly { v50 := calldataload(100) }
            uint256 v51; assembly { v51 := calldataload(200) }
            uint256 v52; assembly { v52 := sload(1) }
            uint256 v53; assembly { v53 := calldataload(0) }
            uint256 v54; assembly { v54 := calldataload(200) }
            uint256 v56; assembly { v56 := sload(3) }
            uint256 v57; assembly { v57 := sload(8) }
            uint256 v59; assembly { v59 := calldatasize() }
            uint256 v60; assembly { v60 := calldatasize() }
            v51 = (v51 ^ (uint256(1) << 3));
            (uint256 v61, uint256 v62, uint256 v63, uint256 v64, uint256 v65) = h1(uint256(1), v49, uint256(1), ((uint256(1) + (v40 / v47)) & v54), v47, v42, v59, uint256(1));
            { uint256 res = ((((((((((((((((((((((((((((((uint256(1) ^ v36) * 1099511628211) ^ v37) * 1099511628211) ^ v38) * 1099511628211) ^ v39) * 1099511628211) ^ v40) * 1099511628211) ^ v43) * 1099511628211) ^ v45) * 1099511628211) ^ v47) * 1099511628211) ^ v50) * 1099511628211) ^ v51) * 1099511628211) ^ v52) * 1099511628211) ^ v53) * 1099511628211) ^ v56) * 1099511628211) ^ v57) * 1099511628211) ^ v59) * 1099511628211) ^ v60; assembly { mstore(0, res) return(0, 32) } }
        }
    }
}
