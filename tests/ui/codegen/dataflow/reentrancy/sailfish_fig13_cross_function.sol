//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 13: cross-function reentrancy found in the wild. During the untrusted
// `transferFrom`, `funcB` rewrites `item_1.creator`, so the fee goes to an attacker-chosen
// address; the fee's recipient also depends on storage another transaction can write.

// CHECK: fn @funcA:
// CHECK: finding: reentrancy cross-function @funcA call#{{[0-9.]+}}: reads slot(0) after the call; reentrant @funcB writes slot(0) (destructive write)
// CHECK: finding: tod @funcA call#{{[0-9.]+}}: recipient depends on slot(0) written by @funcB
interface IERC721 {
    function transferFrom(address from, address to, uint256 tokenId) external;
}

contract CrossFunction {
    struct Item { address payable creator; address game; uint256 fee; }
    Item item_1;

    function funcA(address to, uint256 amt) public {
        to;
        IERC721 erc721 = IERC721(item_1.game);
        erc721.transferFrom(msg.sender, item_1.creator, amt);
        //~^ WARN: possible cross-function reentrancy: `funcA` reads storage after an external call
        item_1.creator.transfer(item_1.fee);
        //~^ NOTE: `funcA` reads `slot(0)` after the call
        //~| WARN: possible transaction-order dependence: the recipient of a transfer in `funcA` depends on storage written by `funcB`
    }

    function funcB(address payable _creator, address _game) public {
        item_1.creator = _creator;
        //~^ NOTE: a reentrant call to `funcB` can write it here (destructive write)
        item_1.game = _game;
    }
}
