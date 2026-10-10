import "../constant_with_cyclic_dependency_file_and_library.sol";

library B {
    uint256 constant VAL = A + 1;
}
