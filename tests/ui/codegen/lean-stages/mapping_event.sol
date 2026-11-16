//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A mapping update hashes its slot with `keccak256` and an event logs it, both exported.

contract LeanStagesMappingEvent {
    mapping(address => uint256) balances;

    event Deposit(address indexed who, uint256 amount);

    function deposit(uint256 amount) external {
        balances[msg.sender] += amount;
        emit Deposit(msg.sender, amount);
    }
}
