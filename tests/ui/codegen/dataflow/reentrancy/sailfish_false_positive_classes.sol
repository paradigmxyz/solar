//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, Sec. VIII-B: false-positive classes that must not be reported.
// (a) `send` and `transfer` forward only the stipend and are not reentrancy vectors.
// (b) A function that only the owner can call cannot be entered by an attacker.
// (c) A variable read after the call that no public function writes is not a hazard.
// (d) A transfer amount written only in the constructor has no transaction order.
// (e) A transfer amount and its transfer both guarded by the owner are not attacker-ordered.

// CHECK-NOT: finding:
contract StipendOnly {
    mapping(address => uint256) balances;

    function withdraw() external {
        payable(msg.sender).transfer(balances[msg.sender]);
        balances[msg.sender] = 0;
    }

    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }
}

contract OwnerOnly {
    address owner;
    uint256 reserve;

    constructor() { owner = msg.sender; }

    function sweep(address target) external {
        require(msg.sender == owner);
        (bool ok, ) = target.call{value: reserve}("");
        require(ok);
        reserve = 0;
    }
}

contract ReadOnlyAfterCall {
    uint256 fee;
    mapping(address => uint256) balances;

    constructor(uint256 initialFee) { fee = initialFee; }

    function withdraw() external {
        uint256 amount = balances[msg.sender];
        balances[msg.sender] = 0;
        (bool ok, ) = msg.sender.call{value: amount}("");
        require(ok && fee < amount);
    }
}

contract ConstructorRate {
    uint256 rate;
    mapping(address => uint256) credits;

    constructor(uint256 initialRate) { rate = initialRate; }

    function redeem() external {
        uint256 amount = credits[msg.sender] * rate;
        credits[msg.sender] = 0;
        payable(msg.sender).transfer(amount);
    }
}

contract OwnerFee {
    address owner;
    uint256 fee;

    constructor() { owner = msg.sender; }

    function setFee(uint256 newFee) external {
        require(msg.sender == owner);
        fee = newFee;
    }

    function collect() external {
        require(msg.sender == owner);
        payable(msg.sender).transfer(fee);
    }
}
