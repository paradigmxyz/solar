//@ codegen-matrix: standard
//@ run-call: DynamicWriterSpills::run => 1

contract DynamicWriterSpills {
    function payload() external pure {
        assembly {
            return(0, 0x600)
        }
    }

    function run() external view returns (uint256 result) {
        assembly {
            let a00 := gas()
            let a01 := gas()
            let a02 := gas()
            let a03 := gas()
            let a04 := gas()
            let a05 := gas()
            let a06 := gas()
            let a07 := gas()
            let a08 := gas()
            let a09 := gas()
            let a10 := gas()
            let a11 := gas()
            let a12 := gas()
            let a13 := gas()
            let a14 := gas()
            let a15 := gas()
            let a16 := gas()
            let a17 := gas()
            let a18 := gas()
            let a19 := gas()
            mstore(0, 0xa878f858)
            if iszero(staticcall(gas(), address(), 0x1c, 4, 0, 0)) { revert(0, 0) }
            returndatacopy(0, 0, returndatasize())
            result := iszero(
                iszero(
                    and(
                        and(and(and(a00, a01), and(a02, a03)), and(and(a04, a05), and(a06, a07))),
                        and(
                            and(and(and(a08, a09), and(a10, a11)), and(and(a12, a13), and(a14, a15))),
                            and(and(a16, a17), and(a18, a19))
                        )
                    )
                )
            )
        }
    }
}
