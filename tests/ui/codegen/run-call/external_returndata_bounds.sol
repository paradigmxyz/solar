//@ run-call: ReturnHarness::staticAggregate() => 1
//@ run-call: ReturnHarness::functionPointerAggregate() => 1
//@ run-call-fail: ReturnHarness::truncated()

contract ReturnProducer {
    struct Inner {
        uint256[2] values;
    }

    struct Outer {
        Inner inner;
        uint256 tag;
    }

    function aggregate() external pure returns (Outer memory out) {
        out.inner.values[0] = 11;
        out.inner.values[1] = 22;
        out.tag = 33;
    }

    function malformed() external pure returns (bytes memory) {
        assembly {
            // A valid head and length word claiming 256 absent payload bytes.
            mstore(0, 0x20)
            mstore(0x20, 0x100)
            return(0, 0x40)
        }
    }
}

contract ReturnHarness {
    function check(ReturnProducer.Outer memory out) internal pure returns (uint256) {
        require(out.inner.values[0] == 11, "first");
        require(out.inner.values[1] == 22, "second");
        require(out.tag == 33, "tag");
        return 1;
    }

    function staticAggregate() external returns (uint256) {
        ReturnProducer producer = new ReturnProducer();
        ReturnProducer.Outer memory out = producer.aggregate();
        return check(out);
    }

    function functionPointerAggregate() external returns (uint256) {
        ReturnProducer producer = new ReturnProducer();
        function () external view returns (ReturnProducer.Outer memory) fn = producer.aggregate;
        ReturnProducer.Outer memory out = fn();
        return check(out);
    }

    function truncated() external returns (uint256) {
        ReturnProducer producer = new ReturnProducer();
        bytes memory value = producer.malformed();
        return value.length;
    }
}
