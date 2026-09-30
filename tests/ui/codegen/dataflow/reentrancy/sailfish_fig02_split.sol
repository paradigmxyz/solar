//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 2 (Example 1): cross-function reentrancy through a destructive write.
// During the call to the first payee, a reentrant `updateSplit` rewrites `splits[id]`, so
// the second payee's share is computed from the attacker's value. The payees, loaded from
// storage the figure elides, are passed by the caller here so that they are untrusted.

// CHECK: fn @splitFunds:
// CHECK: finding: reentrancy cross-function @splitFunds call#{{[0-9.]+}}: reads slot(0)[arg0] after the call; reentrant @updateSplit writes slot(0)[arg0] (destructive write)
// CHECK: finding: tod @splitFunds call#{{[0-9.]+}}: value depends on slot(0)[arg0] written by @updateSplit
contract Split {
    mapping(uint256 => uint256) splits;
    mapping(uint256 => uint256) deposits;

    function updateSplit(uint256 id, uint256 split) public {
        require(split <= 100);
        splits[id] = split;
        //~^ NOTE: a reentrant call to `updateSplit` can write it here (destructive write)
    }

    function splitFunds(uint256 id, address payable a, address payable b) public {
        uint256 depo = deposits[id];
        deposits[id] = 0;
        (bool ok, ) = a.call{value: depo * splits[id] / 100}("");
        //~^ WARN: possible cross-function reentrancy: `splitFunds` reads storage after an external call
        //~| WARN: possible transaction-order dependence: the value of a transfer in `splitFunds` depends on storage written by `updateSplit`
        ok;
        b.transfer(depo * (100 - splits[id]) / 100);
        //~^ NOTE: `splitFunds` reads `slot(0)[arg0]` after the call
    }
}
