//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: encode => true
//@ run-call: encodeLeading => true
//@ run-call: encodeCall => true
//@ run-call: encodeWithSelector => true
//@ run-call: encodeWithSignature => true
//@ run-call: encodePacked => true
//@ run-call: externalCall => true
//@ run-call: tryCall => true
//@ run-call: construct => true
//@ run-call: customError => true
//@ run-call: unchanged => true
// A storage array or struct operand of an ABI encoding is read when the encoder runs, after
// the operands that follow it, as solc does.
struct S {
    uint256 x;
}

struct T {
    uint256[] xs;
    uint256 y;
}

error Err(uint256[] xs, S s, uint256[2] fixedXs, T t, uint256 y);

contract Callee {
    function digest(uint256[] memory xs, S memory s, uint256[2] memory fixedXs, T memory t, uint256 y)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encode(xs, s, fixedXs, t, y));
    }
}

contract Created {
    bytes32 public digest;

    constructor(uint256[] memory xs, S memory s, uint256[2] memory fixedXs, T memory t, uint256 y) {
        digest = keccak256(abi.encode(xs, s, fixedXs, t, y));
    }
}

contract Test {
    uint256[] a;
    S s;
    uint256[2] fixedA;
    T t;

    function h() internal returns (uint256) {
        a.push(7);
        s.x = 9;
        fixedA[1] = 4;
        t.xs.push(5);
        t.y = 6;
        return 0;
    }

    function afterH()
        internal
        pure
        returns (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_)
    {
        xs = new uint256[](1);
        xs[0] = 7;
        s_.x = 9;
        fixedXs[1] = 4;
        t_.xs = new uint256[](1);
        t_.xs[0] = 5;
        t_.y = 6;
    }

    function encode() external returns (bool) {
        bytes memory got = abi.encode(a, s, fixedA, t, h());
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return keccak256(got) == keccak256(abi.encode(xs, s_, fixedXs, t_, 0));
    }

    function encodeLeading() external returns (bool) {
        bytes memory got = abi.encode(a, s, h(), t);
        (uint256[] memory xs, S memory s_,, T memory t_) = afterH();
        return keccak256(got) == keccak256(abi.encode(xs, s_, 0, t_));
    }

    function encodeCall() external returns (bool) {
        bytes memory got = abi.encodeCall(Callee.digest, (a, s, fixedA, t, h()));
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return keccak256(got) == keccak256(abi.encodeCall(Callee.digest, (xs, s_, fixedXs, t_, 0)));
    }

    function encodeWithSelector() external returns (bool) {
        bytes memory got = abi.encodeWithSelector(0x12345678, a, s, fixedA, t, h());
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return keccak256(got) == keccak256(abi.encodeWithSelector(0x12345678, xs, s_, fixedXs, t_, 0));
    }

    function encodeWithSignature() external returns (bool) {
        bytes memory got = abi.encodeWithSignature("f()", a, s, fixedA, t, h());
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return keccak256(got) == keccak256(abi.encodeWithSignature("f()", xs, s_, fixedXs, t_, 0));
    }

    function encodePacked() external returns (bool) {
        bytes memory got = abi.encodePacked(a, fixedA, h());
        (uint256[] memory xs,, uint256[2] memory fixedXs,) = afterH();
        return keccak256(got) == keccak256(abi.encodePacked(xs, fixedXs, uint256(0)));
    }

    function externalCall() external returns (bool) {
        Callee callee = new Callee();
        bytes32 got = callee.digest(a, s, fixedA, t, h());
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return got == keccak256(abi.encode(xs, s_, fixedXs, t_, 0));
    }

    function tryCall() external returns (bool) {
        Callee callee = new Callee();
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        try callee.digest(a, s, fixedA, t, h()) returns (bytes32 got) {
            return got == keccak256(abi.encode(xs, s_, fixedXs, t_, 0));
        } catch {
            return false;
        }
    }

    function construct() external returns (bool) {
        Created created = new Created(a, s, fixedA, t, h());
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        return created.digest() == keccak256(abi.encode(xs, s_, fixedXs, t_, 0));
    }

    function fail() external {
        revert Err(a, s, fixedA, t, h());
    }

    function customError() external returns (bool) {
        (bool ok, bytes memory got) = address(this).call(abi.encodeCall(this.fail, ()));
        (uint256[] memory xs, S memory s_, uint256[2] memory fixedXs, T memory t_) = afterH();
        bytes memory want = abi.encodeWithSelector(Err.selector, xs, s_, fixedXs, t_, 0);
        return !ok && keccak256(got) == keccak256(want);
    }

    function unchanged() external returns (bool) {
        a.push(1);
        a.push(2);
        s.x = 3;
        fixedA[0] = 4;
        t.xs.push(5);
        t.y = 6;
        bytes memory got = abi.encode(a, s, fixedA, t, a.length);
        uint256[] memory xs = new uint256[](2);
        xs[0] = 1;
        xs[1] = 2;
        uint256[2] memory fixedXs;
        fixedXs[0] = 4;
        T memory t_;
        t_.xs = new uint256[](1);
        t_.xs[0] = 5;
        t_.y = 6;
        return keccak256(got) == keccak256(abi.encode(xs, S(3), fixedXs, t_, 2));
    }
}
