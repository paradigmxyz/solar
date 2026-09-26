contract NamedTarget {
    constructor(uint256 left, uint256 right) payable {}
}

contract NamedCreator {
    function deploy() external payable returns (NamedTarget) {
        return (new NamedTarget){value: msg.value}({right: 2, left: 1});
    }
}
