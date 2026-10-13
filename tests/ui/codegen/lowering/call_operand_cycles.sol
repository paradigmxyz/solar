//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Each call consumes its gas, target, and argument pointer in place and takes the
// rest of its operands from fresh pushes. Pushing each constant where it takes the
// place of a word that has to move up settles two words per swap, instead of
// pushing every constant first and then permuting.
// CHECK-LABEL: @module Vaults_runtime
// CHECK: extcodesize
// CHECK: jumpi
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 5
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 4
// CHECK-NEXT: push 68
// CHECK-NEXT: swap 4
// CHECK-NEXT: call
// CHECK: extcodesize
// CHECK: jumpi
// CHECK-NEXT: push 100
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 4
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 6
// CHECK-NEXT: call
interface Token {
    function burnFrom(address from, uint256 amount) external;
}

interface Nft {
    function transferFrom(address from, address to, uint256 id) external;
}

contract Vaults {
    struct Vault {
        Nft nft;
        uint256 id;
        uint256 supply;
        Token token;
    }

    mapping(uint256 => Vault) vaults;

    function join(uint256 key) external {
        Vault memory vault = vaults[key];
        vault.token.burnFrom(msg.sender, vault.supply);
        vault.nft.transferFrom(address(this), msg.sender, vault.id);
    }
}
