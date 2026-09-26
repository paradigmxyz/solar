//@ codegen-matrix: standard
//@ run-call: words => 0x16ed800b3553d170d9b9afe6d01a73447a37318a016996524f15d95a2ceafbc3, 192
//@ run-call: empty => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: dirty 0 => 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
//@ run-call: dirty 1 => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6
//@ run-call: dirty 2 => 0x405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace
//@ run-call: dirty 255 => 0xe08ec2af2cfc251225e1968fd6ca21e4044f129bffa95bac3503be8bdb30a367
//@ run-call: dirty 256 => 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
//@ run-call: aliasing 159 => 0x6e1540171b6c0c960b71a7020d9f60077f6af931a8bbf590da0223dacf75c7af
//@ run-call: aliasing 128 => 0x907b01d808c33f02296a53218680f6472b6c87f90e34f51f187a3d66b01e8dff
//@ run-call: aliasing 160 => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6
//@ run-call: allocationOverrun => 0x405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace
contract Hashes {
    function allocationOverrun() external pure returns (bytes32 hash) {
        uint256[1] memory a;
        uint256[1] memory b;
        assembly {
            mstore(add(a, 32), 1)
            mstore(b, 2)
            hash := keccak256(add(a, 32), 32)
        }
    }

    function words() external pure returns (bytes32 hash, uint256 size) {
        assembly {
            mstore(128, 1)
            mstore(160, 2)
            mstore8(159, 255)
            hash := keccak256(128, 64)
            size := msize()
        }
    }

    function empty() external pure returns (bytes32 hash) {
        assembly { hash := keccak256(not(0), 0) }
    }

    function dirty(uint256 value) external pure returns (bytes32 hash) {
        assembly {
            mstore(128, 1)
            mstore8(159, value)
            hash := keccak256(128, 32)
        }
    }

    function aliasing(uint256 address_) external pure returns (bytes32 hash) {
        assembly {
            mstore(128, 1)
            mstore8(address_, 9)
            hash := keccak256(128, 32)
        }
    }
}
