//@ codegen-matrix: standard
//@ run-call: isOwner 0x000000000000000000000000000000000000beef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => true
//@ run-call: isOwner 0x000000000000000000000000000000000000cafe; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => false
//@ run-call: notOwner 0x000000000000000000000000000000000000beef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => false
//@ run-call: notOwner 0x000000000000000000000000000000000000cafe; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => true
//@ run-call: checkOwner 0x000000000000000000000000000000000000beef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 1
//@ run-call: checkOwner 0x000000000000000000000000000000000000cafe; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 0
//@ run-call: guardOwner 0x000000000000000000000000000000000000beef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 1
//@ run-call-fail: guardOwner 0x000000000000000000000000000000000000cafe; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678]
//@ run-call: sameHolders; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => true
//@ run-call: sameHolders; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => false
//@ run-call: differentHolders; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => false
//@ run-call: differentHolders; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => true
//@ run-call: sameBranch; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 1
//@ run-call: sameBranch; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => 0
//@ run-call: differentBranch; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 0
//@ run-call: differentBranch; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => 1
//@ run-call: isBeef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => true
//@ run-call: isBeef; constructor=[0x000000000000000000000000000000000000cafe, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => false
//@ run-call: notBeef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => false
//@ run-call: notBeef; constructor=[0x000000000000000000000000000000000000cafe, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => true
//@ run-call: branchBeef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 1
//@ run-call: branchBeef; constructor=[0x000000000000000000000000000000000000cafe, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1] => 0
//@ run-call: guardBeef; constructor=[0x000000000000000000000000000000000000beef, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000beef, 0x123456789abcdef012345678] => 1
//@ run-call-fail: guardBeef; constructor=[0x000000000000000000000000000000000000cafe, 0xffffffffffffffffffffffff, 0x000000000000000000000000000000000000cafe, 1]

// A packed slot holds an address under another field, so the word an address check reads has dirty
// bits above the address. The check compares only the low 160 bits.
contract MaskedCompare {
    address owner;
    uint96 stamp;
    address holder;
    uint96 other;

    constructor(address owner_, uint96 stamp_, address holder_, uint96 other_) {
        owner = owner_;
        stamp = stamp_;
        holder = holder_;
        other = other_;
    }

    function isOwner(address account) external view returns (bool) {
        return account == owner;
    }

    function notOwner(address account) external view returns (bool) {
        return account != owner;
    }

    function checkOwner(address account) external view returns (uint256) {
        if (account == owner) return 1;
        return 0;
    }

    function guardOwner(address account) external view returns (uint256) {
        require(account == owner);
        return 1;
    }

    function isBeef() external view returns (bool) {
        return owner == 0x000000000000000000000000000000000000bEEF;
    }

    function notBeef() external view returns (bool) {
        return owner != 0x000000000000000000000000000000000000bEEF;
    }

    function branchBeef() external view returns (uint256) {
        if (owner == 0x000000000000000000000000000000000000bEEF) return 1;
        return 0;
    }

    function guardBeef() external view returns (uint256) {
        require(owner == 0x000000000000000000000000000000000000bEEF);
        return 1;
    }

    function sameHolders() external view returns (bool) {
        return owner == holder;
    }

    function differentHolders() external view returns (bool) {
        return owner != holder;
    }

    function sameBranch() external view returns (uint256) {
        if (owner == holder) return 1;
        return 0;
    }

    function differentBranch() external view returns (uint256) {
        if (owner != holder) return 1;
        return 0;
    }
}
