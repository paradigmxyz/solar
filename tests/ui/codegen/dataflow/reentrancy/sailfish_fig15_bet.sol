//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Fig. 15: transaction-order dependence of the transferred amount. The reward
// depends on `totalBlnc`, which `recordBet` writes. `transfer` forwards only the stipend, so
// the call is not a reentrancy vector.

// CHECK: fn @settleBet:
// CHECK: icall transfer<>{{.*}}; stipend
// CHECK: finding: tod @settleBet call#{{[0-9.]+}}: value depends on {{.*}} written by @recordBet
// CHECK-NOT: finding: reentrancy
contract Bet {
    mapping(address => uint256) userBlnces;
    mapping(bool => uint256) totalBlnc;

    function recordBet(bool bet, uint256 _userAmount) public {
        userBlnces[msg.sender] = _userAmount;
        totalBlnc[bet] = totalBlnc[bet] + _userAmount;
    }

    function settleBet(bool bet) public {
        uint256 reward = (userBlnces[msg.sender] * totalBlnc[!bet]) / totalBlnc[bet];
        uint256 totalWth = reward + userBlnces[msg.sender];
        totalBlnc[!bet] = totalBlnc[!bet] - reward;
        payable(msg.sender).transfer(totalWth);
        //~^ WARN: possible transaction-order dependence: the value of a transfer in `settleBet` depends on storage written by `recordBet`
    }
}
