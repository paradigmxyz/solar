// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Fixed-point, lot and calendar arithmetic. Checked products by constants,
// quotient comparisons and rounding to multiples stay in hot loops; measure
// project workloads separately.
contract DivisionWords {
    /// Charges a 0.3% fee on every hop of a swap route.
    function route(uint256 amount, uint256 hops) external pure returns (uint256) {
        for (uint256 i = 0; i < hops; ++i) {
            amount = amount * 997 / 1000;
        }
        return amount;
    }

    /// Scales six-decimal token amounts to 18-decimal fixed point.
    function toWad(uint256 amount, uint256 step, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += amount * 1e12;
            amount += step;
        }
    }

    /// Counts the amounts that fill a whole lot and those above a cap of whole thousands.
    function lots(uint256 amount, uint256 step, uint256 count)
        external
        pure
        returns (uint256 filled, uint256 capped)
    {
        for (uint256 i = 0; i < count; ++i) {
            if (amount / 1e6 != 0) ++filled;
            if (amount / 1e3 > 5000) ++capped;
            amount += step;
        }
    }

    /// Sums the starts of the weeks that contain a series of timestamps.
    function weekStarts(uint256 time, uint256 step, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += time / 1 weeks * 1 weeks;
            time += step;
        }
    }

    /// Sums the starts of the periods that contain a series of hourly timestamps.
    function periodStarts(uint256 time, uint256 period, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += time / period * period;
            time += 1 hours;
        }
    }

    /// Converts 18-decimal amounts to whole units in two nine-decimal steps.
    function units(uint256 amount, uint256 step, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += amount / 1e9 / 1e9;
            amount += step;
        }
    }

    /// Deducts a 0.3% fee from each amount of a series.
    function afterFees(uint256 amount, uint256 step, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += amount - amount * 30 / 10_000;
            amount += step;
        }
    }

    /// Keeps all but one of a variable number of shares of each amount.
    function keep(uint256 amount, uint256 shares, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += amount - amount / shares;
            amount += 1 ether;
        }
    }

    /// Sums the seconds into the day of a series of timestamps.
    function secondsIntoDay(uint256 time, uint256 step, uint256 count) external pure returns (uint256 total) {
        for (uint256 i = 0; i < count; ++i) {
            total += time - time / 1 days * 1 days;
            time += step;
        }
    }

    /// Counts the amounts of a series that are whole lots of 1,000.
    function wholeLots(uint256 amount, uint256 step, uint256 count) external pure returns (uint256 whole) {
        for (uint256 i = 0; i < count; ++i) {
            if (amount / 1000 * 1000 == amount) ++whole;
            amount += step;
        }
    }
}
