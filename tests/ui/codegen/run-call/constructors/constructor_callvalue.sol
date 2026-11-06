//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorCallvalue::rejectsExplicit; value=1 => true
//@ run-call: ConstructorCallvalue::rejectsImplicit; value=1 => true
//@ run-call: ConstructorCallvalue::rejectsSynthetic; value=1 => true
//@ run-call: ConstructorCallvalue::rejectsInherited; value=1 => true
//@ run-call: ConstructorCallvalue::acceptsPayable; value=1 => true

contract ExplicitNonpayable {
    constructor(uint256) {}
}

contract ImplicitNonpayable {}

contract SyntheticNonpayable {
    uint256 value = 1;
}

contract PayableBase {
    constructor() payable {}
}

contract InheritedNonpayable is PayableBase {}

contract ExplicitPayable {
    constructor() payable {}
}

contract ConstructorCallvalue {
    function rejectsExplicit() external payable returns (bool) {
        bytes memory code = abi.encodePacked(type(ExplicitNonpayable).creationCode, abi.encode(1));
        return deploy(code) == address(0);
    }

    function rejectsImplicit() external payable returns (bool) {
        return deploy(type(ImplicitNonpayable).creationCode) == address(0);
    }

    function rejectsSynthetic() external payable returns (bool) {
        return deploy(type(SyntheticNonpayable).creationCode) == address(0);
    }

    function rejectsInherited() external payable returns (bool) {
        return deploy(type(InheritedNonpayable).creationCode) == address(0);
    }

    function acceptsPayable() external payable returns (bool) {
        address deployed = deploy(type(ExplicitPayable).creationCode);
        return deployed != address(0) && deployed.balance == msg.value;
    }

    function deploy(bytes memory code) private returns (address deployed) {
        assembly {
            deployed := create(callvalue(), add(code, 32), mload(code))
        }
    }
}
