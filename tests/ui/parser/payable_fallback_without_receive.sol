contract P1 {
    fallback() external payable {}
}

contract P2 is P1 {
    receive() external payable {}
}

contract P3 {
    fallback() external payable {}

    receive() external payable {}
}

contract P4 is P1 {}
