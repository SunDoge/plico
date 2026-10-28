//! Controlled scalar input shared by sliders and future numeric editors.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NumericRange {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    /// Zero means continuous pointer input, arrows use 1/100 of the range.
    pub step: f64,
    pub vertical: bool,
    pub reversed: bool,
}
impl NumericRange {
    pub fn valid(self) -> bool {
        [self.min, self.max, self.value, self.step]
            .iter()
            .all(|v| v.is_finite())
            && self.min <= self.max
            && (self.max - self.min).is_finite()
            && self.step >= 0.
    }
    pub fn normalize(self, value: f64) -> f64 {
        if !self.valid() {
            return if self.min.is_finite() { self.min } else { 0. };
        }
        if value.is_nan() {
            return self.min;
        }
        let value = value.clamp(self.min, self.max);
        if value == self.min || value == self.max || self.step == 0. {
            return value;
        }
        let steps = (value - self.min) / self.step;
        if !steps.is_finite() {
            return value;
        }
        (self.min + steps.round() * self.step).clamp(self.min, self.max)
    }
    pub fn fraction(self) -> f64 {
        if self.max == self.min {
            0.
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0., 1.)
        }
    }
    pub fn increment(self) -> f64 {
        if self.step > 0. {
            self.step
        } else {
            (self.max - self.min) / 100.
        }
    }
}
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct SliderResponse {
    pub changed: bool,
    pub value: f64,
}
