pragma solidity ^0.8.20;
// An ambiguous struct nested 17 levels deep still makes the layout unknown: a raw `sstore`
// onto `after_` must not pass as disjoint from the slots `d` takes.
interface IOld { struct Item { uint256 amount; } }
interface INew { struct Item { address owner; uint8 kind; } }
struct D1 { D2 d; }
struct D2 { D3 d; }
struct D3 { D4 d; }
struct D4 { D5 d; }
struct D5 { D6 d; }
struct D6 { D7 d; }
struct D7 { D8 d; }
struct D8 { D9 d; }
struct D9 { D10 d; }
struct D10 { D11 d; }
struct D11 { D12 d; }
struct D12 { D13 d; }
struct D13 { D14 d; }
struct D14 { D15 d; }
struct D15 { D16 d; }
struct D16 { D17 d; }
struct D17 { INew.Item it; }
contract Deep {
    D1 d;
    uint256 public after_;
    /// @custom:ensures after_ == old(after_)
    function clobber() external { assembly { sstore(1, 7) } }
}
