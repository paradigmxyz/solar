//@compile-flags: -Zcodegen --emit=bin-runtime

interface SliceToken {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
}

interface SliceReceiver {
    function onData(SliceToken token, uint256 amount, bytes calldata data) external;
}

contract CalldataSliceCallStack {
    mapping(SliceToken => uint256) public fees;

    event Called(SliceReceiver indexed receiver, SliceToken indexed token, uint256 amount);

    function execute(
        SliceReceiver receiver,
        SliceToken token,
        uint256 amount,
        bytes calldata data
    ) external {
        uint256 balance = token.balanceOf(address(this));
        emit Called(receiver, token, amount);
        token.transfer(address(receiver), amount);
        receiver.onData(token, amount, data);
        if (balance + getFee(token, amount) > token.balanceOf(address(this))) revert();
    }

    function getFee(SliceToken token, uint256 amount) public view returns (uint256) {
        if (fees[token] == 0) return 0;
        return (amount * fees[token]) / 10_000;
    }
}
