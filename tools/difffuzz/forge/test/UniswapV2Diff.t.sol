// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, Vm} from "forge-std/Test.sol";

interface IToken {
    function balanceOf(address) external view returns (uint256);
    function transfer(address, uint256) external returns (bool);
}

interface IFactory {
    function createPair(address, address) external returns (address);
    function setFeeTo(address) external;
}

interface IPair {
    function DOMAIN_SEPARATOR() external view returns (bytes32);
    function nonces(address) external view returns (uint256);
    function getReserves() external view returns (uint112, uint112, uint32);
}

/// Two builds of Uniswap v2-core, by solc and by the compiler under test, each with its own
/// factory, pair and tokens: the tokens sit at addresses in the same order in both, so token0
/// and token1 match. Every step runs on both and must have the same success, return data and
/// logs (pair and token addresses aside), and leave the same balances, reserves, price
/// accumulators and kLast.
abstract contract Builds is Test {
    uint256 constant N = 2;
    string[N] internal names = ["solc", "solar"];
    string[N] internal outs = ["lib/uniswap-v2/out-solc", "lib/uniswap-v2/out-solar"];

    IFactory[N] internal factory;
    IPair[N] internal pair;
    IToken[N] internal tokenA;
    IToken[N] internal tokenB;
    // A fresh, code-less recipient per build, so balances compare one to one.
    address[N] internal user;

    function _deploy() internal {
        for (uint256 i; i < N; i++) {
            address a = _addr(0x0A, i);
            address b = _addr(0x0B, i);
            deployCodeTo(string.concat(outs[i], "/ERC20.sol/ERC20.json"), abi.encode(1e40), a);
            deployCodeTo(string.concat(outs[i], "/ERC20.sol/ERC20.json"), abi.encode(1e40), b);
            tokenA[i] = IToken(a);
            tokenB[i] = IToken(b);
            factory[i] = IFactory(
                deployCode(string.concat(outs[i], "/UniswapV2Factory.sol/UniswapV2Factory.json"), abi.encode(address(this)))
            );
            pair[i] = IPair(factory[i].createPair(a, b));
            user[i] = _addr(0x0C, i);
            vm.label(address(pair[i]), string.concat("pair-", names[i]));
        }
    }

    /// An address that no amount equals, with prefix in its top byte so that the tokens keep
    /// their order in both builds.
    function _addr(uint256 prefix, uint256 i) internal pure returns (address) {
        uint256 low = uint256(keccak256(abi.encode(prefix, i))) & ((1 << 152) - 1);
        return address(uint160((prefix << 152) | low));
    }

    /// Replaces the build's own addresses in data with build 0's, so that return data and logs
    /// that name them compare.
    function _norm(uint256 i, bytes memory data) internal view returns (bytes memory) {
        if (i == 0) return data;
        address[4] memory from = [address(pair[i]), address(tokenA[i]), address(tokenB[i]), user[i]];
        address[4] memory to = [address(pair[0]), address(tokenA[0]), address(tokenB[0]), user[0]];
        for (uint256 w; w + 32 <= data.length; w += 32) {
            bytes32 word;
            assembly {
                word := mload(add(add(data, 0x20), w))
            }
            for (uint256 k; k < 4; k++) {
                if (word == bytes32(uint256(uint160(from[k])))) {
                    bytes32 repl = bytes32(uint256(uint160(to[k])));
                    assembly {
                        mstore(add(add(data, 0x20), w), repl)
                    }
                }
            }
        }
        return data;
    }

    /// Runs one call per build and returns the first difference, or an empty string.
    function _run(address[N] memory target, bytes[N] memory data) internal returns (string memory) {
        bool[N] memory ok;
        bytes[N] memory ret;
        bytes32[N] memory logs;
        for (uint256 i; i < N; i++) {
            vm.recordLogs();
            (ok[i], ret[i]) = target[i].call(data[i]);
            Vm.Log[] memory l = vm.getRecordedLogs();
            bytes memory flat;
            for (uint256 k; k < l.length; k++) {
                for (uint256 t; t < l[k].topics.length; t++) {
                    flat = bytes.concat(flat, l[k].topics[t]);
                }
                flat = bytes.concat(flat, l[k].data);
            }
            logs[i] = keccak256(_norm(i, flat));
            ret[i] = _norm(i, ret[i]);
        }
        if (ok[1] != ok[0]) return "success";
        if (keccak256(ret[1]) != keccak256(ret[0])) return "return data";
        if (logs[1] != logs[0]) return "logs";
        return _state();
    }

    function _state() internal view returns (string memory) {
        bytes[N] memory s;
        for (uint256 i; i < N; i++) {
            address p = address(pair[i]);
            (uint112 r0, uint112 r1, uint32 t) = pair[i].getReserves();
            s[i] = bytes.concat(
                abi.encode(r0, r1, t, IToken(p).balanceOf(user[i]), IToken(p).balanceOf(address(this))),
                abi.encode(
                    _read(p, "totalSupply()"),
                    _read(p, "price0CumulativeLast()"),
                    _read(p, "price1CumulativeLast()"),
                    _read(p, "kLast()")
                ),
                abi.encode(
                    tokenA[i].balanceOf(p), tokenB[i].balanceOf(p), tokenA[i].balanceOf(user[i]), tokenB[i].balanceOf(user[i])
                )
            );
        }
        return keccak256(s[0]) == keccak256(s[1]) ? "" : "state";
    }

    function _read(address p, string memory sig) internal view returns (uint256 v) {
        (, bytes memory r) = p.staticcall(abi.encodeWithSignature(sig));
        v = abi.decode(r, (uint256));
    }

    function _pairCall(bytes memory data) internal returns (string memory) {
        return _run([address(pair[0]), address(pair[1])], [data, data]);
    }

    // The steps, each applied to both builds.

    function _addLiquidity(uint256 a, uint256 b, bool toUser) internal returns (string memory) {
        for (uint256 i; i < N; i++) {
            tokenA[i].transfer(address(pair[i]), a);
            tokenB[i].transfer(address(pair[i]), b);
        }
        return _run(
            [address(pair[0]), address(pair[1])],
            [
                abi.encodeWithSignature("mint(address)", toUser ? user[0] : address(this)),
                abi.encodeWithSignature("mint(address)", toUser ? user[1] : address(this))
            ]
        );
    }

    function _removeLiquidity(uint256 liquidity) internal returns (string memory) {
        for (uint256 i; i < N; i++) {
            IToken(address(pair[i])).transfer(address(pair[i]), liquidity);
        }
        return _run(
            [address(pair[0]), address(pair[1])],
            [abi.encodeWithSignature("burn(address)", user[0]), abi.encodeWithSignature("burn(address)", user[1])]
        );
    }

    function _swap(uint256 amountIn, bool aIn, uint256 out0, uint256 out1) internal returns (string memory) {
        for (uint256 i; i < N; i++) {
            (aIn ? tokenA[i] : tokenB[i]).transfer(address(pair[i]), amountIn);
        }
        return _run(
            [address(pair[0]), address(pair[1])],
            [
                abi.encodeWithSignature("swap(uint256,uint256,address,bytes)", out0, out1, user[0], ""),
                abi.encodeWithSignature("swap(uint256,uint256,address,bytes)", out0, out1, user[1], "")
            ]
        );
    }

    /// Output for an exact input, by the v2 periphery's formula, then moved by delta, so that
    /// both sides of the K check are reached.
    function _quote(uint256 amountIn, bool aIn, int8 delta) internal view returns (uint256 out0, uint256 out1) {
        (uint112 r0, uint112 r1,) = pair[0].getReserves();
        (uint256 rin, uint256 rout) = aIn ? (uint256(r0), uint256(r1)) : (uint256(r1), uint256(r0));
        if (rin == 0 || rout == 0) return (0, 0);
        uint256 withFee = amountIn * 997;
        uint256 out = withFee * rout / (rin * 1000 + withFee);
        if (delta < 0) {
            uint256 d = uint256(-int256(delta));
            out = out < d ? 0 : out - d;
        } else {
            out += uint256(int256(delta));
        }
        return aIn ? (uint256(0), out) : (out, uint256(0));
    }

    function _permit(uint256 key, uint256 value, uint256 deadline, bool wrongNonce) internal returns (string memory) {
        address owner = vm.addr(key);
        bytes[N] memory data;
        for (uint256 i; i < N; i++) {
            uint256 nonce = pair[i].nonces(owner) + (wrongNonce ? 1 : 0);
            bytes32 structHash = keccak256(
                abi.encode(
                    keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"),
                    owner,
                    user[i],
                    value,
                    nonce,
                    deadline
                )
            );
            bytes32 digest = keccak256(abi.encodePacked("\x19\x01", pair[i].DOMAIN_SEPARATOR(), structHash));
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, digest);
            data[i] = abi.encodeWithSignature(
                "permit(address,address,uint256,uint256,uint8,bytes32,bytes32)", owner, user[i], value, deadline, v, r, s
            );
        }
        return _run([address(pair[0]), address(pair[1])], data);
    }
}

contract UniswapV2Diff is Builds {
    // Amounts from v2-core's own tests (test/UniswapV2Pair.spec.ts): 1, 5 and 10 tokens of 18
    // decimals for liquidity and swaps, and the 1000-wei minimum liquidity edge.
    uint256[] public fixtureA = [1e18, 5e18, 10e18, 1000, 1001];
    uint256[] public fixtureB = [4e18, 10e18, 5e18, 1000, 1001];

    function setUp() public {
        _deploy();
        vm.warp(1_700_000_000);
    }

    function testFuzz_mintBurn(uint256 a, uint256 b, uint256 burnPart, bool toUser) public {
        a = bound(a, 0, type(uint112).max);
        b = bound(b, 0, type(uint112).max);
        assertEq(_addLiquidity(a, b, toUser), "");
        uint256 lp = IToken(address(pair[0])).balanceOf(address(this));
        assertEq(_removeLiquidity(bound(burnPart, 0, lp)), "");
    }

    function testFuzz_swap(uint256 a, uint256 b, uint256 amountIn, bool aIn, int8 delta, uint32 wait) public {
        a = bound(a, 1001, 1e30);
        b = bound(b, 1001, 1e30);
        assertEq(_addLiquidity(a, b, false), "");
        vm.warp(block.timestamp + wait);
        amountIn = bound(amountIn, 0, 1e30);
        (uint256 o0, uint256 o1) = _quote(amountIn, aIn, delta);
        assertEq(_swap(amountIn, aIn, o0, o1), "");
        assertEq(_pairCall(abi.encodeWithSignature("sync()")), "");
    }

    function testFuzz_feeOn(uint256 a, uint256 b, uint256 amountIn, uint256 more) public {
        for (uint256 i; i < N; i++) {
            factory[i].setFeeTo(address(0xFEE));
        }
        a = bound(a, 1001, 1e30);
        b = bound(b, 1001, 1e30);
        assertEq(_addLiquidity(a, b, false), "");
        amountIn = bound(amountIn, 1, 1e30);
        (uint256 o0, uint256 o1) = _quote(amountIn, true, 0);
        assertEq(_swap(amountIn, true, o0, o1), "");
        more = bound(more, 0, 1e30);
        assertEq(_addLiquidity(more, more, true), "");
    }

    function testFuzz_permit(uint256 key, uint256 value, uint256 deadline, bool wrongNonce) public {
        key = bound(key, 1, 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364140);
        assertEq(_permit(key, value, deadline, wrongNonce), "");
        assertEq(_permit(key, value, deadline, false), "");
    }

    function testFuzz_rawCalldata(bytes4 selector, bytes memory tail) public {
        assertEq(_pairCall(abi.encodePacked(selector, tail)), "");
    }

    /// v2-core's swap test: 5 and 10 tokens of liquidity, 1 token in, 1662497915624478906 out.
    function test_swapVector() public {
        assertEq(_addLiquidity(5e18, 10e18, false), "");
        assertEq(_swap(1e18, true, 0, 1662497915624478906), "");
        (uint112 r0, uint112 r1,) = pair[1].getReserves();
        assertEq(uint256(r0), 6e18);
        assertEq(uint256(r1), 10e18 - 1662497915624478906);
    }
}

/// Random sequences of mints, burns, swaps, syncs, skims, donations, permits and waits on both
/// builds in lockstep; the invariant is that no step told them apart.
contract Lockstep is Builds {
    string public diff;
    uint256 public steps;

    constructor() {
        _deploy();
    }

    function _note(string memory d) internal {
        steps++;
        if (bytes(diff).length == 0 && bytes(d).length != 0) diff = d;
    }

    function mint(uint256 a, uint256 b, bool toUser) external {
        _note(_addLiquidity(a % 1e30, b % 1e30, toUser));
    }

    function burn(uint256 part) external {
        uint256 lp = IToken(address(pair[0])).balanceOf(address(this));
        _note(_removeLiquidity(part % (lp + 1)));
    }

    function swap(uint256 amountIn, bool aIn, int8 delta) external {
        amountIn %= 1e30;
        (uint256 o0, uint256 o1) = _quote(amountIn, aIn, delta);
        _note(_swap(amountIn, aIn, o0, o1));
    }

    function sync() external {
        _note(_pairCall(abi.encodeWithSignature("sync()")));
    }

    function skim() external {
        _note(
            _run(
                [address(pair[0]), address(pair[1])],
                [abi.encodeWithSignature("skim(address)", user[0]), abi.encodeWithSignature("skim(address)", user[1])]
            )
        );
    }

    function donate(uint256 a, bool toA) external {
        for (uint256 i; i < N; i++) {
            (toA ? tokenA[i] : tokenB[i]).transfer(address(pair[i]), a % 1e30);
        }
        _note(_state());
    }

    function permit(uint256 key, uint256 value) external {
        _note(_permit(key % 1000 + 1, value, type(uint256).max, false));
    }

    function wait(uint32 dt) external {
        vm.warp(block.timestamp + dt);
    }
}

contract UniswapV2Lockstep is Test {
    Lockstep internal h;

    function setUp() public {
        vm.warp(1_700_000_000);
        h = new Lockstep();
        targetContract(address(h));
    }

    function invariant_buildsAgree() public view {
        assertEq(h.diff(), "");
        assertGt(h.steps() + 1, 0);
    }
}
