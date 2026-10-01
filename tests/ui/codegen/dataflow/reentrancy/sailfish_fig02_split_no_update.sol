//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Sec. III-C: Fig. 2 without `updateSplit`. No public function writes `splits`,
// so reading it after the call is not a hazard and nothing is reported. As in the Fig. 2
// test, the caller chooses the payees.

// CHECK: fn @splitFunds:
// CHECK-NOT: finding:
contract SplitNoUpdate {
    mapping(uint256 => uint256) splits;
    mapping(uint256 => uint256) deposits;

    function splitFunds(uint256 id, address payable a, address payable b) public {
        uint256 depo = deposits[id];
        deposits[id] = 0;
        (bool ok, ) = a.call{value: depo * splits[id] / 100}("");
        ok;
        b.transfer(depo * (100 - splits[id]) / 100);
    }
}
