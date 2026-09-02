//@ codegen-matrix: standard
//@ run-call: storageIndex 0x101 => 2

contract DirtyStorageArrayIndex {
    uint256[] sarr;

    function inject(uint256 raw) internal pure returns (uint8 x) {
        assembly {
            x := raw
        }
    }

    function storageIndex(uint256 raw) external returns (uint256) {
        sarr.push(1);
        sarr.push(2);
        sarr.push(3);
        return sarr[inject(raw)];
    }
}
