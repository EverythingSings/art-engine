//! Two-dimensional scalar field with toroidal wrapping and clamped values.
//!
//! A `Field` stores `width * height` f64 values in the range [0, 1] using
//! row-major layout. Coordinate access uses toroidal (wrap-around) addressing
//! so negative and overflowing indices are valid.

use crate::error::EngineError;

/// A 2D scalar field with values clamped to [0, 1] and toroidal coordinate wrapping.
#[derive(Debug, Clone)]
pub struct Field {
    width: usize,
    height: usize,
    data: Vec<f64>,
}

impl Field {
    /// Creates a zero-filled field of the given dimensions.
    ///
    /// Returns `EngineError::InvalidDimensions` if either dimension is zero
    /// or if `width * height` overflows `usize`.
    pub fn new(width: usize, height: usize) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        Ok(Self {
            width,
            height,
            data: vec![0.0; len],
        })
    }

    /// Creates a field filled with `value`, clamped to [0, 1].
    ///
    /// Returns `EngineError::InvalidDimensions` if either dimension is zero
    /// or if `width * height` overflows `usize`.
    pub fn filled(width: usize, height: usize, value: f64) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let len = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        Ok(Self {
            width,
            height,
            data: vec![value.clamp(0.0, 1.0); len],
        })
    }

    /// Field width in cells.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Field height in cells.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Read-only access to the underlying row-major data.
    pub fn data(&self) -> &[f64] {
        &self.data
    }

    /// Converts signed coordinates to a flat index using toroidal wrapping.
    fn index(&self, x: isize, y: isize) -> usize {
        let w = self.width as isize;
        let h = self.height as isize;
        let xi = x.rem_euclid(w) as usize;
        let yi = y.rem_euclid(h) as usize;
        yi * self.width + xi
    }

    /// Gets the value at `(x, y)` with toroidal wrapping.
    pub fn get(&self, x: isize, y: isize) -> f64 {
        self.data[self.index(x, y)]
    }

    /// Sets the value at `(x, y)` with toroidal wrapping. The value is clamped to [0, 1].
    pub fn set(&mut self, x: isize, y: isize, value: f64) {
        let idx = self.index(x, y);
        self.data[idx] = value.clamp(0.0, 1.0);
    }

    /// Mutable access to the underlying row-major data.
    ///
    /// Values written here bypass the [0, 1] clamping. Engine hot paths
    /// that manage their own invariants can use this for performance.
    pub fn data_mut(&mut self) -> &mut [f64] {
        &mut self.data
    }

    /// Creates a field from a pre-built data vector, validating that
    /// `data.len() == width * height`.
    ///
    /// Values are **not** clamped; the caller is responsible for ensuring
    /// they lie in [0, 1].
    pub fn from_data(width: usize, height: usize, data: Vec<f64>) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::InvalidDimensions);
        }
        let expected = width
            .checked_mul(height)
            .ok_or(EngineError::InvalidDimensions)?;
        if data.len() != expected {
            return Err(EngineError::DimensionMismatch {
                lhs_w: width,
                lhs_h: height,
                rhs_w: data.len(),
                rhs_h: 1,
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Element-wise addition of two fields, clamped to [0, 1].
    ///
    /// Returns `EngineError::DimensionMismatch` if the fields differ in size.
    pub fn add(&self, other: &Field) -> Result<Field, EngineError> {
        if self.width != other.width || self.height != other.height {
            return Err(EngineError::DimensionMismatch {
                lhs_w: self.width,
                lhs_h: self.height,
                rhs_w: other.width,
                rhs_h: other.height,
            });
        }
        Ok(Field {
            width: self.width,
            height: self.height,
            data: self
                .data
                .iter()
                .zip(other.data.iter())
                .map(|(a, b)| (a + b).clamp(0.0, 1.0))
                .collect(),
        })
    }

    /// Element-wise multiplication of two fields, clamped to [0, 1].
    ///
    /// Returns `EngineError::DimensionMismatch` if the fields differ in size.
    pub fn multiply(&self, other: &Field) -> Result<Field, EngineError> {
        if self.width != other.width || self.height != other.height {
            return Err(EngineError::DimensionMismatch {
                lhs_w: self.width,
                lhs_h: self.height,
                rhs_w: other.width,
                rhs_h: other.height,
            });
        }
        Ok(Field {
            width: self.width,
            height: self.height,
            data: self
                .data
                .iter()
                .zip(other.data.iter())
                .map(|(a, b)| (a * b).clamp(0.0, 1.0))
                .collect(),
        })
    }

    /// In-place element-wise addition, clamped to [0, 1].
    ///
    /// Returns `EngineError::DimensionMismatch` if the fields differ in size.
    pub fn add_assign(&mut self, other: &Field) -> Result<(), EngineError> {
        if self.width != other.width || self.height != other.height {
            return Err(EngineError::DimensionMismatch {
                lhs_w: self.width,
                lhs_h: self.height,
                rhs_w: other.width,
                rhs_h: other.height,
            });
        }
        self.data
            .iter_mut()
            .zip(other.data.iter())
            .for_each(|(a, b)| *a = (*a + b).clamp(0.0, 1.0));
        Ok(())
    }

    /// In-place element-wise multiplication, clamped to [0, 1].
    ///
    /// Returns `EngineError::DimensionMismatch` if the fields differ in size.
    pub fn multiply_assign(&mut self, other: &Field) -> Result<(), EngineError> {
        if self.width != other.width || self.height != other.height {
            return Err(EngineError::DimensionMismatch {
                lhs_w: self.width,
                lhs_h: self.height,
                rhs_w: other.width,
                rhs_h: other.height,
            });
        }
        self.data
            .iter_mut()
            .zip(other.data.iter())
            .for_each(|(a, b)| *a = (*a * b).clamp(0.0, 1.0));
        Ok(())
    }

    /// In-place scaling of all values by `factor`, clamped to [0, 1].
    pub fn scale_assign(&mut self, factor: f64) {
        self.data
            .iter_mut()
            .for_each(|v| *v = (*v * factor).clamp(0.0, 1.0));
    }

    /// Scales all values by `factor`, clamped to [0, 1].
    pub fn scale(&self, factor: f64) -> Field {
        Field {
            width: self.width,
            height: self.height,
            data: self
                .data
                .iter()
                .map(|v| (v * factor).clamp(0.0, 1.0))
                .collect(),
        }
    }

    /// Iterates over all cells yielding `(x, y, value)` in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, f64)> + '_ {
        self.data.iter().enumerate().map(|(i, &v)| {
            let x = i % self.width;
            let y = i / self.width;
            (x, y, v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Constructor tests --

    #[test]
    fn new_creates_zero_filled_field() {
        let field = Field::new(4, 3).unwrap();
        assert_eq!(field.width(), 4);
        assert_eq!(field.height(), 3);
        assert_eq!(field.data().len(), 12);
        assert!(field.data().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn new_with_zero_width_returns_error() {
        let result = Field::new(0, 5);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EngineError::InvalidDimensions
        ));
    }

    #[test]
    fn new_with_zero_height_returns_error() {
        let result = Field::new(5, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EngineError::InvalidDimensions
        ));
    }

    #[test]
    fn new_with_both_zero_returns_error() {
        let result = Field::new(0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn filled_creates_correct_values() {
        let field = Field::filled(3, 2, 0.7).unwrap();
        assert!(field.data().iter().all(|&v| (v - 0.7).abs() < f64::EPSILON));
    }

    #[test]
    fn filled_clamps_value_above_one() {
        let field = Field::filled(2, 2, 1.5).unwrap();
        assert!(field.data().iter().all(|&v| (v - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn filled_clamps_value_below_zero() {
        let field = Field::filled(2, 2, -0.3).unwrap();
        assert!(field.data().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn filled_with_zero_dimension_returns_error() {
        assert!(Field::filled(0, 3, 0.5).is_err());
        assert!(Field::filled(3, 0, 0.5).is_err());
    }

    // -- get/set with positive indices --

    #[test]
    fn get_and_set_with_positive_indices() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(2, 3, 0.42);
        assert!((field.get(2, 3) - 0.42).abs() < f64::EPSILON);
    }

    #[test]
    fn set_at_origin() {
        let mut field = Field::new(3, 3).unwrap();
        field.set(0, 0, 0.99);
        assert!((field.get(0, 0) - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn set_at_max_valid_index() {
        let mut field = Field::new(5, 5).unwrap();
        field.set(4, 4, 0.5);
        assert!((field.get(4, 4) - 0.5).abs() < f64::EPSILON);
    }

    // -- Toroidal wrapping --

    #[test]
    fn get_wraps_negative_x() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(3, 0, 0.8);
        // x = -1 should wrap to x = 3 (width = 4)
        assert!((field.get(-1, 0) - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn get_wraps_negative_y() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(0, 3, 0.6);
        // y = -1 should wrap to y = 3 (height = 4)
        assert!((field.get(0, -1) - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn get_wraps_overflow_x() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(1, 0, 0.3);
        // x = 5 should wrap to x = 1 (5 % 4 = 1)
        assert!((field.get(5, 0) - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn get_wraps_overflow_y() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(0, 2, 0.9);
        // y = 6 should wrap to y = 2 (6 % 4 = 2)
        assert!((field.get(0, 6) - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn set_with_negative_indices_wraps() {
        let mut field = Field::new(3, 3).unwrap();
        field.set(-1, -1, 0.77);
        // (-1, -1) wraps to (2, 2) for 3x3 field
        assert!((field.get(2, 2) - 0.77).abs() < f64::EPSILON);
    }

    #[test]
    fn set_with_large_negative_wraps() {
        let mut field = Field::new(4, 4).unwrap();
        field.set(-5, -9, 0.33);
        // -5 rem_euclid 4 = 3, -9 rem_euclid 4 = 3
        assert!((field.get(3, 3) - 0.33).abs() < f64::EPSILON);
    }

    // -- Value clamping --

    #[test]
    fn set_clamps_value_above_one() {
        let mut field = Field::new(2, 2).unwrap();
        field.set(0, 0, 2.5);
        assert!((field.get(0, 0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_clamps_value_below_zero() {
        let mut field = Field::new(2, 2).unwrap();
        field.set(0, 0, -0.5);
        assert!(field.get(0, 0) == 0.0);
    }

    // -- Arithmetic operations --

    #[test]
    fn add_two_fields_element_wise() {
        let a = Field::filled(2, 2, 0.3).unwrap();
        let b = Field::filled(2, 2, 0.4).unwrap();
        let c = a.add(&b).unwrap();
        assert!(c.data().iter().all(|&v| (v - 0.7).abs() < f64::EPSILON));
    }

    #[test]
    fn add_clamps_to_one() {
        let a = Field::filled(2, 2, 0.8).unwrap();
        let b = Field::filled(2, 2, 0.5).unwrap();
        let c = a.add(&b).unwrap();
        assert!(c.data().iter().all(|&v| (v - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn add_returns_error_on_dimension_mismatch() {
        let a = Field::new(2, 3).unwrap();
        let b = Field::new(3, 2).unwrap();
        let result = a.add(&b);
        assert!(matches!(result, Err(EngineError::DimensionMismatch { .. })));
    }

    #[test]
    fn multiply_two_fields_element_wise() {
        let a = Field::filled(2, 2, 0.5).unwrap();
        let b = Field::filled(2, 2, 0.6).unwrap();
        let c = a.multiply(&b).unwrap();
        assert!(c.data().iter().all(|&v| (v - 0.3).abs() < f64::EPSILON));
    }

    #[test]
    fn multiply_with_zero_field_yields_zero() {
        let a = Field::filled(2, 2, 0.8).unwrap();
        let b = Field::new(2, 2).unwrap(); // all zeros
        let c = a.multiply(&b).unwrap();
        assert!(c.data().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn multiply_returns_error_on_dimension_mismatch() {
        let a = Field::new(2, 2).unwrap();
        let b = Field::new(3, 3).unwrap();
        let result = a.multiply(&b);
        assert!(matches!(result, Err(EngineError::DimensionMismatch { .. })));
    }

    #[test]
    fn scale_multiplies_all_values() {
        let field = Field::filled(2, 2, 0.4).unwrap();
        let scaled = field.scale(0.5);
        assert!(scaled
            .data()
            .iter()
            .all(|&v| (v - 0.2).abs() < f64::EPSILON));
    }

    #[test]
    fn scale_clamps_above_one() {
        let field = Field::filled(2, 2, 0.8).unwrap();
        let scaled = field.scale(2.0);
        assert!(scaled
            .data()
            .iter()
            .all(|&v| (v - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn scale_clamps_below_zero_for_negative_factor() {
        let field = Field::filled(2, 2, 0.5).unwrap();
        let scaled = field.scale(-1.0);
        assert!(scaled.data().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn scale_does_not_mutate_original() {
        let field = Field::filled(2, 2, 0.4).unwrap();
        let _scaled = field.scale(2.0);
        assert!(field.data().iter().all(|&v| (v - 0.4).abs() < f64::EPSILON));
    }

    // -- Iterator --

    #[test]
    fn iter_yields_all_triples_in_row_major_order() {
        let mut field = Field::new(3, 2).unwrap();
        field.set(0, 0, 0.1);
        field.set(1, 0, 0.2);
        field.set(2, 0, 0.3);
        field.set(0, 1, 0.4);
        field.set(1, 1, 0.5);
        field.set(2, 1, 0.6);

        let triples: Vec<(usize, usize, f64)> = field.iter().collect();
        assert_eq!(triples.len(), 6);
        assert_eq!(triples[0], (0, 0, 0.1));
        assert_eq!(triples[1], (1, 0, 0.2));
        assert_eq!(triples[2], (2, 0, 0.3));
        assert_eq!(triples[3], (0, 1, 0.4));
        assert_eq!(triples[4], (1, 1, 0.5));
        assert_eq!(triples[5], (2, 1, 0.6));
    }

    #[test]
    fn iter_on_empty_field_yields_nothing_for_1x1() {
        let field = Field::new(1, 1).unwrap();
        let triples: Vec<_> = field.iter().collect();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0], (0, 0, 0.0));
    }

    // -- Clone --

    #[test]
    fn clone_produces_independent_copy() {
        let mut original = Field::new(3, 3).unwrap();
        original.set(1, 1, 0.5);
        let clone = original.clone();
        assert!((clone.get(1, 1) - 0.5).abs() < f64::EPSILON);

        // Mutating original should not affect clone
        original.set(1, 1, 0.9);
        assert!((clone.get(1, 1) - 0.5).abs() < f64::EPSILON);
    }

    // -- Overflow guard --

    #[test]
    fn new_with_overflow_dimensions_returns_error() {
        let result = Field::new(usize::MAX, 2);
        assert!(result.is_err());
    }

    #[test]
    fn filled_with_overflow_dimensions_returns_error() {
        let result = Field::filled(usize::MAX, 2, 0.5);
        assert!(result.is_err());
    }

    // -- In-place operations --

    #[test]
    fn add_assign_modifies_in_place() {
        let mut a = Field::filled(2, 2, 0.3).unwrap();
        let b = Field::filled(2, 2, 0.4).unwrap();
        a.add_assign(&b).unwrap();
        assert!(a.data().iter().all(|&v| (v - 0.7).abs() < f64::EPSILON));
    }

    #[test]
    fn add_assign_returns_error_on_mismatch() {
        let mut a = Field::new(2, 2).unwrap();
        let b = Field::new(3, 3).unwrap();
        assert!(matches!(
            a.add_assign(&b),
            Err(EngineError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn multiply_assign_modifies_in_place() {
        let mut a = Field::filled(2, 2, 0.5).unwrap();
        let b = Field::filled(2, 2, 0.6).unwrap();
        a.multiply_assign(&b).unwrap();
        assert!(a.data().iter().all(|&v| (v - 0.3).abs() < f64::EPSILON));
    }

    #[test]
    fn multiply_assign_returns_error_on_mismatch() {
        let mut a = Field::new(2, 2).unwrap();
        let b = Field::new(3, 3).unwrap();
        assert!(matches!(
            a.multiply_assign(&b),
            Err(EngineError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn scale_assign_modifies_in_place() {
        let mut field = Field::filled(2, 2, 0.4).unwrap();
        field.scale_assign(0.5);
        assert!(field.data().iter().all(|&v| (v - 0.2).abs() < f64::EPSILON));
    }

    // -- data_mut --

    #[test]
    fn data_mut_allows_direct_write() {
        let mut field = Field::new(2, 2).unwrap();
        field.data_mut()[0] = 0.42;
        assert!((field.get(0, 0) - 0.42).abs() < f64::EPSILON);
    }

    // -- from_data --

    #[test]
    fn from_data_creates_field_from_vec() {
        let data = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let field = Field::from_data(3, 2, data).unwrap();
        assert_eq!(field.width(), 3);
        assert_eq!(field.height(), 2);
        assert!((field.get(0, 0) - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn from_data_rejects_wrong_length() {
        let data = vec![0.1, 0.2, 0.3];
        let result = Field::from_data(2, 2, data);
        assert!(result.is_err());
    }

    #[test]
    fn from_data_rejects_zero_dimensions() {
        let result = Field::from_data(0, 5, vec![]);
        assert!(result.is_err());
    }

    // -- Property-based tests --

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Strategy for field dimensions (1..=64 to keep tests fast).
        fn dimension() -> impl Strategy<Value = usize> {
            1_usize..=64
        }

        /// Strategy for arbitrary f64 values (including out-of-range).
        fn any_value() -> impl Strategy<Value = f64> {
            prop::num::f64::ANY.prop_filter("must not be NaN", |v| !v.is_nan())
        }

        /// Strategy for coordinate values that can be negative or large.
        fn any_coord() -> impl Strategy<Value = isize> {
            -1000_isize..=1000
        }

        proptest! {
            #[test]
            fn get_after_set_returns_clamped_value(
                w in dimension(),
                h in dimension(),
                x in any_coord(),
                y in any_coord(),
                v in any_value(),
            ) {
                let mut field = Field::new(w, h).unwrap();
                field.set(x, y, v);
                let got = field.get(x, y);
                let expected = v.clamp(0.0, 1.0);
                prop_assert!(
                    (got - expected).abs() < f64::EPSILON,
                    "get({x}, {y}) = {got}, expected {expected} (set value {v})"
                );
            }

            #[test]
            fn toroidal_equivalence(
                w in dimension(),
                h in dimension(),
                x in any_coord(),
                y in any_coord(),
                v in any_value(),
            ) {
                let iw = w as isize;
                let ih = h as isize;
                let mut field = Field::new(w, h).unwrap();
                field.set(x, y, v);
                // Value at (x, y) should equal value at (x + w, y + h)
                prop_assert!(
                    (field.get(x, y) - field.get(x + iw, y + ih)).abs() < f64::EPSILON,
                    "toroidal equivalence failed for ({x}, {y}) in {w}x{h}"
                );
            }

            #[test]
            fn add_is_commutative(
                w in dimension(),
                h in dimension(),
                data_a in prop::collection::vec(0.0_f64..=1.0, 1..=4096),
                data_b in prop::collection::vec(0.0_f64..=1.0, 1..=4096),
            ) {
                // Use the generated data to fill fields up to w*h values
                let mut a = Field::new(w, h).unwrap();
                let mut b = Field::new(w, h).unwrap();
                let n = w * h;
                for i in 0..n {
                    let x = (i % w) as isize;
                    let y = (i / w) as isize;
                    a.set(x, y, data_a[i % data_a.len()]);
                    b.set(x, y, data_b[i % data_b.len()]);
                }
                let ab = a.add(&b).unwrap();
                let ba = b.add(&a).unwrap();
                for (va, vb) in ab.data().iter().zip(ba.data().iter()) {
                    prop_assert!(
                        (va - vb).abs() < f64::EPSILON,
                        "add not commutative: {va} vs {vb}"
                    );
                }
            }
        }
    }
}
