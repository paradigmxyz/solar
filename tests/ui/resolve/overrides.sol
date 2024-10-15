abstract contract MyAbstractTest {
    function setUp() public virtual;
}

contract MyTestSuite is MyAbstractTest {
    function setUp() public virtual override {}
    function other() public {}
}

contract MyTest is MyTestSuite {
    function setUp() public virtual override {}
}
