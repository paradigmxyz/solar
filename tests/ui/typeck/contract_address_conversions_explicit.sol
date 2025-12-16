//@compile-flags: -Ztypeck

contract NoReceive {}

contract WithReceive {
    receive() external payable {}
}

contract WithPayableFallback {
    fallback() external payable {}
}

contract WithNonPayableFallback {
    fallback() external {}
}

contract Test {
    function testContractToAddress(
        NoReceive c1,
        WithReceive c2,
        WithPayableFallback c3
    ) public pure {
        // All contracts can convert to address
        address a1 = address(c1);
        address a2 = address(c2);
        address a3 = address(c3);
    }

    function testContractToAddressPayable(
        NoReceive c1,
        WithReceive c2,
        WithPayableFallback c3,
        WithNonPayableFallback c4
    ) public pure {
        // Only contracts with receive or payable fallback can convert to address payable
        address payable p1 = payable(c1); //~ ERROR: cannot convert
        address payable p2 = payable(c2); // ok
        address payable p3 = payable(c3); // ok
        address payable p4 = payable(c4); //~ ERROR: cannot convert
    }

    function testAddressToContract(address addr, address payable paddr) public pure {
        // Non-payable address can convert to contracts without receive/payable fallback
        NoReceive c1 = NoReceive(addr); // ok
        WithNonPayableFallback c4 = WithNonPayableFallback(addr); // ok

        // Non-payable address CANNOT convert to contracts with receive/payable fallback
        WithReceive c2 = WithReceive(addr); //~ ERROR: invalid explicit type conversion
        WithPayableFallback c3 = WithPayableFallback(addr); //~ ERROR: invalid explicit type conversion

        // Payable address can convert to any contract
        WithReceive c5 = WithReceive(paddr); // ok
        WithPayableFallback c6 = WithPayableFallback(paddr); // ok
        NoReceive c7 = NoReceive(paddr); // ok
        WithNonPayableFallback c8 = WithNonPayableFallback(paddr); // ok
    }
}
