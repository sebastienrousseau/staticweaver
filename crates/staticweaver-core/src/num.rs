// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `f64` rounding for the numeric filters (`round`, `ceil`, `floor`).
//!
//! `std` provides these as inherent methods; `core` does not, because
//! they come from the platform maths library. Without `std` the
//! `exact` module below computes them from an integer truncation,
//! which is exact for every `f64` (values of magnitude 2^52 or more,
//! and non-finite values, are already integral and pass through).

#[cfg(feature = "std")]
pub(crate) fn round(x: f64) -> f64 {
    x.round()
}

#[cfg(feature = "std")]
pub(crate) fn ceil(x: f64) -> f64 {
    x.ceil()
}

#[cfg(feature = "std")]
pub(crate) fn floor(x: f64) -> f64 {
    x.floor()
}

#[cfg(feature = "std")]
pub(crate) fn fract(x: f64) -> f64 {
    x.fract()
}

#[cfg(not(feature = "std"))]
pub(crate) use exact::{ceil, floor, fract, round};

#[cfg(any(test, not(feature = "std")))]
mod exact {
    /// 2^52: every `f64` at or beyond this magnitude is an integer.
    const INTEGRAL: f64 = 4_503_599_627_370_496.0;

    fn trunc(x: f64) -> f64 {
        if !x.is_finite() || x >= INTEGRAL || x <= -INTEGRAL {
            x
        } else {
            // |x| < 2^52, so the cast is exact both ways.
            (x as i64) as f64
        }
    }

    /// Rounds half away from zero, like `f64::round`.
    pub(crate) fn round(x: f64) -> f64 {
        let t = trunc(x);
        let d = x - t;
        if d >= 0.5 {
            t + 1.0
        } else if d <= -0.5 {
            t - 1.0
        } else {
            t
        }
    }

    pub(crate) fn ceil(x: f64) -> f64 {
        let t = trunc(x);
        if t < x {
            t + 1.0
        } else {
            t
        }
    }

    pub(crate) fn floor(x: f64) -> f64 {
        let t = trunc(x);
        if t > x {
            t - 1.0
        } else {
            t
        }
    }

    pub(crate) fn fract(x: f64) -> f64 {
        x - trunc(x)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every helper agrees with the `std` method it replaces, up
        /// to the sign of zero (which the filters render the same).
        #[test]
        fn exact_helpers_match_std() {
            let samples = [
                0.0,
                -0.0,
                0.3,
                -0.3,
                0.5,
                -0.5,
                0.499_999_999_999_999_94,
                1.5,
                -1.5,
                2.5,
                -2.5,
                2.4,
                -2.6,
                1e15 + 0.5,
                -1e15 - 0.5,
                4.5e15,
                -4.5e15,
                1e300,
                -1e300,
                123.456,
                -123.456,
            ];
            for x in samples {
                assert!(round(x) == x.round(), "round({x})");
                assert!(ceil(x) == x.ceil(), "ceil({x})");
                assert!(floor(x) == x.floor(), "floor({x})");
                assert!(fract(x) == x.fract(), "fract({x})");
            }
            assert!(round(f64::INFINITY).is_infinite());
            assert!(fract(f64::NAN).is_nan());
        }
    }
}
