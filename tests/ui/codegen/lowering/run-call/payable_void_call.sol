//@ codegen-matrix: standard
//@ run-call: Caller::run => 7

// A void payable external function called after a local emit.

contract Callee {
    event Approval(address indexed owner, address indexed account, uint256 indexed id);

    uint256 public stored;

    function approve(address account, uint256 id) public payable {
        emit Approval(msg.sender, account, id);
        stored = id;
    }
}

contract Caller {
    event Approval(address indexed owner, address indexed account, uint256 indexed id);

    function run() external returns (uint256) {
        Callee c = new Callee();
        emit Approval(address(this), address(0xBEEF), 7);
        c.approve(address(0xBEEF), 7);
        return c.stored();
    }
}
