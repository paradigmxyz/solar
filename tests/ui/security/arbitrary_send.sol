//@ compile-flags: -Zsecurity --emit=bin-runtime --allow=2264

// The analysis traces each ether-carrying call's recipient backward through the
// MIR. A recipient derived from an argument or calldata means anyone can choose
// where funds go (fund-drain risk) and is reported; sending to `msg.sender` —
// the ordinary withdraw pattern — lowers to CALLER and is not flagged (the
// false-positive guard).

contract ArbitrarySend {
    mapping(address => uint256) balances;

    // Recipient is an arbitrary argument: anyone can drain to any address.
    function payAnyone(address payable to, uint256 amount) external {
        to.transfer(amount); //~ WARN: ether sent to an attacker-controlled address
    }

    // Recipient forwarded from calldata via a low-level call: controlled.
    function forward(address to) external payable {
        (bool ok, ) = to.call{value: msg.value}(""); //~ WARN: ether sent to an attacker-controlled address
        require(ok);
    }

    // Recipient is `msg.sender`: the safe withdraw pattern, no finding.
    function withdraw() external {
        uint256 amount = balances[msg.sender];
        balances[msg.sender] = 0;
        payable(msg.sender).transfer(amount);
    }
}
