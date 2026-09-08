//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[mir] filecheck: --check-prefix=SEMANTIC
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: concrete => "ConcreteTarget"
//@ run-call: abstractContract => "AbstractTarget"
//@ run-call: interfaceContract => "InterfaceTarget"
//@ run-call: libraryContract => "LibraryTarget"
//@ run-call: parenthesized => "ConcreteTarget"
//@ run-call: constantName => "ConcreteTarget"
//@ run-call: longName => "ContractNameLongerThanThirtyTwoBytes"
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

    // SEMANTIC-LABEL: fn @concrete()
    // SEMANTIC: [[DATA:v[0-9]+]] = memory_object_data memorybytes,
    // SEMANTIC-NEXT: mstore [[DATA]], 0x436f6e6372657465546172676574000000000000000000000000000000000000
    // CHECK-LABEL: fn @concrete()
    // CHECK: mstore 128, 32
    // CHECK-NEXT: mstore 160, 14
    // CHECK-NEXT: mstore 192, 0x436f6e6372657465546172676574000000000000000000000000000000000000
    // CHECK-NEXT: returndata 128, 96
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
