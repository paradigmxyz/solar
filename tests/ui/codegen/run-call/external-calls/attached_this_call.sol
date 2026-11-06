//@ codegen-matrix: standard
//@ run-call: attachedThis => true
//@ run-call: attachedValue => 19

library ThisMethods {
    function selfAddress(AttachedThisCall receiver) internal pure returns (address) {
        return address(receiver);
    }

    function addValue(AttachedThisCall receiver, uint256 value) internal pure returns (uint256) {
        return receiver.value() + value;
    }
}

contract AttachedThisCall {
    using ThisMethods for AttachedThisCall;

    bool private constructorReceiver;

    constructor() {
        constructorReceiver = this.selfAddress() == address(this);
    }

    function attachedThis() external view returns (bool) {
        return constructorReceiver && this.selfAddress() == address(this);
    }

    function attachedValue() external view returns (uint256) {
        return this.addValue(7);
    }

    function value() external pure returns (uint256) {
        return 12;
    }
}
