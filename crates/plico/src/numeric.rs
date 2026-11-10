//! Controlled scalar input shared by sliders and future numeric editors.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NumericRange {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    /// The upper thumb of a range slider; `None` for a single-value slider.
    pub high: Option<f64>,
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
            && self.high.is_none_or(|high| {
                high.is_finite() && self.min <= self.value && self.value <= high && high <= self.max
            })
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
        self.fraction_of(self.value)
    }
    pub fn fraction_of(self, value: f64) -> f64 {
        if self.max == self.min {
            0.
        } else {
            ((value - self.min) / (self.max - self.min)).clamp(0., 1.)
        }
    }
    /// Snap a value to the step grid and clamp it into the range.
    pub fn normalized(self, value: f64) -> f64 {
        self.normalize(value)
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
    /// The upper thumb value for a range slider.
    pub high: Option<f64>,
}

/// Result from a composed [`crate::UiContext::value_control`].
#[derive(Debug, Clone, PartialEq)]
pub struct ValueControlResponse<R> {
    pub inner: R,
    /// True when this declaration consumed a new numeric input value.
    pub input: bool,
    pub changed: bool,
    pub value: f64,
    pub focused: bool,
}
