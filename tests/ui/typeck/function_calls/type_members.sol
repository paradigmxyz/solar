//@compile-flags: -Ztypeck

type Pointer is uint256;

library PointerLib {
    function offset(Pointer ptr, uint256 by) internal pure returns (Pointer next) {
        ptr;
        by;
    }
}

interface Executor {
    function execute(uint256 value) external returns (bytes4 magic);
}

contract C {
    function libraryFunctionPointer() public pure {
        function(Pointer, uint256) internal pure returns (Pointer) fn = PointerLib.offset;
        fn;
    }

    function interfaceFunctionSelector() public pure returns (bytes4) {
        return Executor.execute.selector;
    }
}
