use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub count: usize,
    pub row_height: f32,
    pub start: usize,
    pub end: usize,
}
impl Description {
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if value.count > 1_000_000
            || !value.row_height.is_finite()
            || value.row_height <= 0.
            || value.row_height > 16384.
            || value.count as f64 * value.row_height as f64 > 100_000_000.
            || value.start > value.end
            || value.end > value.count
        {
            return Err("invalid virtual list range or extent".into());
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ranges_that_cannot_index_the_submitted_rows() {
        for json in [
            r#"{"count":2,"row_height":40,"start":2,"end":1}"#,
            r#"{"count":2,"row_height":40,"start":0,"end":3}"#,
            r#"{"count":2,"row_height":0,"start":0,"end":2}"#,
            r#"{"count":1000001,"row_height":40,"start":0,"end":2}"#,
        ] {
            assert!(Description::parse(json).is_err(), "{json}");
        }
        assert!(
            Description::parse(r#"{"count":10000,"row_height":40,"start":198,"end":208}"#).is_ok()
        );
    }
}
