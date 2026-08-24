//@ run-call: concrete() => "ConcreteTarget"
//@ run-call: abstractContract() => "AbstractTarget"
//@ run-call: interfaceContract() => "InterfaceTarget"
//@ run-call: libraryContract() => "LibraryTarget"
//@ run-call: parenthesized() => "ConcreteTarget"
//@ run-call: constantName() => "ConcreteTarget"
//@ run-call: longName() => "ContractNameLongerThanThirtyTwoBytes"
// ported-from: test/libsolidity/semanticTests/metaTypes/name_other_contract.sol

abstract contract AbstractTarget {
    function probe() external pure virtual returns (uint256);
}

interface InterfaceTarget {
    function probe() external pure returns (uint256);
}

library LibraryTarget {}

contract ConcreteTarget {}

contract ContractNameLongerThanThirtyTwoBytes {}

contract ContractNames {
    string private constant NAME = type(ConcreteTarget).name;

    function concrete() external pure returns (string memory) {
        return type(ConcreteTarget).name;
    }

    function abstractContract() external pure returns (string memory) {
        return type(AbstractTarget).name;
    }

    function interfaceContract() external pure returns (string memory) {
        return type(InterfaceTarget).name;
    }

    function libraryContract() external pure returns (string memory) {
        return type(LibraryTarget).name;
    }

    function parenthesized() external pure returns (string memory) {
        return (type(ConcreteTarget)).name;
    }

    function constantName() external pure returns (string memory) {
        return NAME;
    }

    function longName() external pure returns (string memory) {
        return type(ContractNameLongerThanThirtyTwoBytes).name;
    }
}
