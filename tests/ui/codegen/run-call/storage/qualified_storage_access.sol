//@ codegen-matrix: standard
//@ run-call: QualifiedStorage::signers 0x0000000000000000000000000000000000000007 => [0x0000000000000000000000000000000000000007]

contract BaseQualifiedStorage {
    uint128 internal packed;
    address[] internal values;
}

contract QualifiedStorage is BaseQualifiedStorage {
    function signers(address signer) external returns (address[] memory) {
        BaseQualifiedStorage.values.push(signer);
        return BaseQualifiedStorage.values;
    }
}
