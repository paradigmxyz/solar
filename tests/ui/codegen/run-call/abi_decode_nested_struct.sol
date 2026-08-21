//@ run-call: Probe::roundTrip() => 1
//@ run-call: Probe2::outer() => 1
//@ run-call: Probe3::run() => 1

// `abi.decode` of a struct that nests other structs and a dynamic member
// must rebuild every nested field, not skew the head offsets.

contract Probe {
    struct Key {
        address c0;
        address c1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    struct Params {
        int24 tickLower;
        int24 tickUpper;
        int256 liquidityDelta;
        bytes32 salt;
    }

    struct CallbackData {
        address sender;
        Key key;
        Params params;
        bytes hookData;
        bool settleUsingBurn;
        bool takeClaims;
    }

    function roundTrip() external pure returns (uint256) {
        CallbackData memory input = CallbackData({
            sender: address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496),
            key: Key({
                c0: address(0x15cF58144EF33af1e14b5208015d11F9143E27b9),
                c1: address(0x212224D2F2d262cd093eE13240ca4873fcCBbA3C),
                fee: 3000,
                tickSpacing: 60,
                hooks: address(0)
            }),
            params: Params({tickLower: -120, tickUpper: 120, liquidityDelta: 1e18, salt: 0}),
            hookData: "",
            settleUsingBurn: false,
            takeClaims: false
        });
        bytes memory blob = abi.encode(input);
        CallbackData memory out = abi.decode(blob, (CallbackData));
        require(out.sender == input.sender, "sender");
        require(out.key.c0 == input.key.c0, "c0");
        require(out.key.c1 == input.key.c1, "c1");
        require(out.key.fee == input.key.fee, "fee");
        require(out.key.tickSpacing == input.key.tickSpacing, "ts");
        require(out.key.hooks == input.key.hooks, "hooks");
        require(out.params.tickLower == -120, "tl");
        require(out.params.tickUpper == 120, "tu");
        require(out.params.liquidityDelta == 1e18, "ld");
        require(out.hookData.length == 0, "hd");
        require(!out.settleUsingBurn && !out.takeClaims, "flags");
        bytes32 idIn;
        bytes32 idOut;
        Key memory kin = input.key;
        Key memory kout = out.key;
        assembly {
            idIn := keccak256(kin, 0xa0)
            idOut := keccak256(kout, 0xa0)
        }
        require(idIn == idOut, "id mismatch");
        return 1;
    }
}

type Currency is address;

contract Probe2 {
    struct Key {
        Currency c0;
        Currency c1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    struct CallbackData {
        address sender;
        Key key;
        int256 delta;
        bytes hookData;
    }

    function outer() external view returns (uint256) {
        CallbackData memory input = CallbackData({
            sender: address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496),
            key: Key({
                c0: Currency.wrap(address(0x15cF58144EF33af1e14b5208015d11F9143E27b9)),
                c1: Currency.wrap(address(0x212224D2F2d262cd093eE13240ca4873fcCBbA3C)),
                fee: 3000,
                tickSpacing: 60,
                hooks: address(0)
            }),
            delta: -120,
            hookData: ""
        });
        bytes32 idIn;
        Key memory kin = input.key;
        assembly {
            idIn := keccak256(kin, 0xa0)
        }
        bytes memory blob = abi.encode(input);
        bytes32 idOut = this.inner(blob);
        require(idIn == idOut, "id mismatch");
        return 1;
    }

    function inner(bytes calldata data) external pure returns (bytes32 idOut) {
        CallbackData memory out = abi.decode(data, (CallbackData));
        Key memory kout = out.key;
        assembly {
            idOut := keccak256(kout, 0xa0)
        }
    }
}

contract Probe3 {
    struct Key {
        Currency c0;
        Currency c1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    struct Params {
        int24 tickLower;
        int24 tickUpper;
        int256 liquidityDelta;
        bytes32 salt;
    }

    function idOfKeyOnly(Key memory k, uint160 x) external pure returns (bytes32 id) {
        x;
        assembly {
            id := keccak256(k, 0xa0)
        }
    }

    function idOfKeyMixed(Key memory k, Params memory p, bytes calldata h)
        external
        pure
        returns (bytes32 id)
    {
        p;
        h;
        assembly {
            id := keccak256(k, 0xa0)
        }
    }

    function run() external view returns (uint256) {
        Key memory k = Key({
            c0: Currency.wrap(address(0x15cF58144EF33af1e14b5208015d11F9143E27b9)),
            c1: Currency.wrap(address(0x212224D2F2d262cd093eE13240ca4873fcCBbA3C)),
            fee: 3000,
            tickSpacing: 60,
            hooks: address(0)
        });
        Params memory p = Params({tickLower: -120, tickUpper: 120, liquidityDelta: 1e18, salt: 0});
        bytes32 idLocal;
        assembly {
            idLocal := keccak256(k, 0xa0)
        }
        bytes32 a = this.idOfKeyOnly(k, 1);
        bytes32 b = this.idOfKeyMixed(k, p, "");
        require(a == idLocal, "keyOnly id");
        require(b == idLocal, "mixed id");
        return 1;
    }
}
