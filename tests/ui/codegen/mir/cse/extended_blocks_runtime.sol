//@ codegen-matrix: standard
//@ run-call: aliasedElement 0 => 5
//@ run-call: aliasedElement 1 => 1
//@ run-call: aliasedSlotInBranch true, 0 => 5
//@ run-call: aliasedSlotInBranch true, 1 => 1
//@ run-call: aliasedSlotInBranch false, 0 => 0
//@ run-call: callChangesSlot => 7
contract StorageCseExtendedBlocks {
    uint256[2] words;

    function aliasedElement(uint256 i) external returns (uint256) {
        words[0] = 1;
        words[i] = 5;
        return words[0];
    }

    function aliasedSlotInBranch(bool condition, uint256 slot) external returns (uint256 result) {
        assembly {
            sstore(0, 1)
            switch condition
            case 0 { sstore(0, 3) }
            default {
                sstore(slot, 5)
                result := sload(0)
                sstore(0, 2)
            }
        }
    }

    function callChangesSlot() external returns (uint256) {
        words[0] = 1;
        this.setWord(7);
        return words[0];
    }

    function setWord(uint256 value) external {
        words[0] = value;
    }
}
