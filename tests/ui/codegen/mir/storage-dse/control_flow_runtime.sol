//@ codegen-matrix: standard
//@ run-call: write true => 2
//@ run-call: write false => 3
//@ run-call: observed true => 1
//@ run-call: observed false => 0
//@ run-call: pack 255, 128 => 33023
//@ run-call: pack 1, 2 => 513
//@ run-call: preserve 255, 128 => 18446744073709519103
//@ run-call: observer => 1
//@ run-call: changingSlot => 2
contract StorageControlFlow {
    function changingSlot() external returns (uint256 result) {
        assembly {
            for { let i := 0 } 1 { i := add(i, 1) } {
                sstore(i, 1)
                if iszero(lt(i, 2)) { break }
                sstore(i, 2)
            }
            result := sload(0)
        }
    }
    function observer() external returns (uint256 old) {
        word = 1;
        old = this.readWord();
        word = 2;
    }
    function readWord() external view returns (uint256) { return word; }

    uint256 word;
    uint8 a;
    uint8 b;
    function write(bool choice) external returns (uint256) {
        word = 1;
        if (choice) word = 2; else word = 3;
        return word;
    }
    function observed(bool choice) external returns (uint256 old) {
        word = 1;
        if (choice) { old = word; word = 2; } else word = 3;
    }
    function pack(uint8 x, uint8 y) external returns (uint256) {
        a = x;
        b = y;
        return uint256(a) | (uint256(b) << 8);
    }
    function preserve(uint8 x, uint8 y) external returns (uint256) {
        word = type(uint64).max;
        word = (word & ~uint256(255)) | x;
        word = (word & ~(uint256(255) << 8)) | (uint256(y) << 8);
        return word;
    }
}
