//@ compile-flags: -Zdataflow=reentrancy
//@ filecheck:
// Sailfish, App. II-A: two transaction-order dependences. The owner can front-run a purchase
// and raise the price, which changes the refund; and a sale's payout depends on the supply,
// which another trader's purchase changes.

// CHECK-LABEL: :OwnerPrice ===
// CHECK: finding: tod @buy {{.*}}: value depends on slot(1) written by @setPrice
// CHECK-LABEL: :SupplyPrice ===
// CHECK: finding: tod @sell {{.*}}: value depends on slot(0) written by @buy
contract OwnerPrice {
    address owner;
    uint256 price;

    constructor() { owner = msg.sender; }

    function setPrice(uint256 newPrice) external {
        require(msg.sender == owner);
        price = newPrice;
    }

    function buy() external payable {
        require(msg.value >= price);
        payable(msg.sender).transfer(msg.value - price);
        //~^ WARN: possible transaction-order dependence: the value of a transfer in `buy` depends on storage written by `setPrice`
    }
}

contract SupplyPrice {
    uint256 totalSupply;
    mapping(address => uint256) balances;

    function buy() external payable {
        balances[msg.sender] += msg.value;
        totalSupply += msg.value;
    }

    function sell(uint256 amount) external {
        balances[msg.sender] -= amount;
        payable(msg.sender).transfer(amount * 1e18 / (totalSupply + 1));
        //~^ WARN: possible transaction-order dependence: the value of a transfer in `sell` depends on storage written by `buy`
    }
}
