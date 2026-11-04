//! App-owned Fluent resources and ICU4X formatters, initialized once per locale.
//! No global locale, system clock, browser APIs, or renderer dependencies.
use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
pub use fluent_bundle::{FluentArgs, FluentValue};
use icu_calendar::Gregorian;
use icu_datetime::{FixedCalendarDateTimeFormatter, fieldsets};
pub use icu_decimal::input::Decimal;
use icu_locale::fallback::LocaleFallbacker;
use icu_locale_core::Locale;
use icu_provider_adapters::fallback::LocaleFallbackProvider;
use icu_provider_blob::BlobDataProvider;
use plico::{CalendarDate, Localizer};
use snafu::Snafu;
use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
};
use writeable::Writeable;

/// Optional built-in widget messages. Applications explicitly include only the
/// languages they ship, alongside their own resources.
pub mod widgets {
    pub const EN: &str = include_str!("../locales/en.ftl");
    pub const ZH_CN: &str = include_str!("../locales/zh-CN.ftl");
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("invalid locale {locale}: {details}"))]
    Locale { locale: String, details: String },
    #[snafu(display("invalid Fluent resource: {details}"))]
    Resource { details: String },
    #[snafu(display("ICU4X data for {locale}: {details}"))]
    Data { locale: String, details: String },
    #[snafu(display("missing Fluent message {id}"))]
    Missing { id: String },
    #[snafu(display("cannot format Fluent message {id}: {details}"))]
    Format { id: String, details: String },
}

/// Parsed messages and cached formatting rules for one explicit locale.
/// Include only the message files your app uses. Missing messages can fall back
/// to another catalog; missing ICU data is an initialization error.
pub struct Catalog {
    locale: String,
    bundle: FluentBundle<FluentResource>,
    fallback: Option<Arc<Catalog>>,
    number: icu_decimal::DecimalFormatter,
    month: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::YM>,
    date: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::YMD>,
    weekdays: [String; 7],
    errors: Mutex<Vec<String>>,
}
impl Catalog {
    /// Use the bundled, trimmed English/Chinese ICU4X data. Resources are parsed
    /// here, never during `show`. This constructor never guesses a system locale.
    #[cfg(feature = "embedded-data")]
    pub fn new<'a>(
        locale: &str,
        resources: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, Error> {
        Self::with_data(locale, resources, include_bytes!("../data/icu4x.postcard"))
    }
    /// Use a custom blob, typically generated for only your app's languages and
    /// formatters. Disable default features to omit the bundled blob entirely.
    pub fn with_data<'a>(
        locale: &str,
        resources: impl IntoIterator<Item = &'a str>,
        data: &'static [u8],
    ) -> Result<Self, Error> {
        let invalid = |details: String| Error::Locale {
            locale: locale.into(),
            details,
        };
        let icu_locale: Locale = locale.parse().map_err(|e| invalid(format!("{e}")))?;
        // Fluent needs the language identifier; Unicode formatting preferences
        // remain on the full ICU locale (e.g. en-US-u-nu-latn).
        let lang = icu_locale
            .id
            .to_string()
            .parse()
            .map_err(|e: unic_langid::LanguageIdentifierError| invalid(e.to_string()))?;
        let fail = |details: String| Error::Data {
            locale: locale.into(),
            details,
        };
        let blob =
            BlobDataProvider::try_new_from_static_blob(data).map_err(|e| fail(e.to_string()))?;
        let fallbacker = LocaleFallbacker::try_new_with_buffer_provider(&blob)
            .map_err(|e| fail(e.to_string()))?;
        let provider = LocaleFallbackProvider::new(blob, fallbacker);
        let number = icu_decimal::DecimalFormatter::try_new_with_buffer_provider(
            &provider,
            icu_locale.clone().into(),
            Default::default(),
        )
        .map_err(|e| fail(e.to_string()))?;
        let month = FixedCalendarDateTimeFormatter::try_new_with_buffer_provider(
            &provider,
            icu_locale.clone().into(),
            fieldsets::YM::long(),
        )
        .map_err(|e| fail(e.to_string()))?;
        let date = FixedCalendarDateTimeFormatter::try_new_with_buffer_provider(
            &provider,
            icu_locale.clone().into(),
            fieldsets::YMD::medium(),
        )
        .map_err(|e| fail(e.to_string()))?;
        let weekday = FixedCalendarDateTimeFormatter::<Gregorian, _>::try_new_with_buffer_provider(
            &provider,
            icu_locale.into(),
            fieldsets::E::short(),
        )
        .map_err(|e| fail(e.to_string()))?;
        let weekdays = std::array::from_fn(|day| {
            let date = icu_calendar::Date::try_new_gregorian(2026, 1, 4 + day as u8)
                .expect("valid Sunday-first reference week");
            weekday.format(&date).write_to_string().into_owned()
        });
        let mut bundle = FluentBundle::new_concurrent(vec![lang]);
        for source in resources {
            let resource = FluentResource::try_new(source.to_owned()).map_err(|(_, errors)| {
                Error::Resource {
                    details: format!("{errors:?}"),
                }
            })?;
            bundle
                .add_resource(resource)
                .map_err(|errors| Error::Resource {
                    details: format!("{errors:?}"),
                })?;
        }
        Ok(Self {
            locale: locale.into(),
            bundle,
            fallback: None,
            number,
            month,
            date,
            weekdays,
            errors: Mutex::new(Vec::new()),
        })
    }
    /// Fallbacks are immutable after construction, so ownership cannot form a cycle.
    pub fn with_fallback(mut self, fallback: Arc<Catalog>) -> Self {
        self.fallback = Some(fallback);
        self
    }
    pub fn locale(&self) -> &str {
        &self.locale
    }
    /// Fluent handles variables, selects and locale-sensitive plural categories.
    /// Formatting errors are returned; an invalid translation is not silently
    /// replaced by another language. Fluent's bidi isolation remains enabled.
    pub fn format<'a>(
        &'a self,
        id: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Result<Cow<'a, str>, Error> {
        if let Some(message) = self.bundle.get_message(id) {
            let pattern = message
                .value()
                .ok_or_else(|| Error::Missing { id: id.into() })?;
            let mut errors = Vec::new();
            let output = self.bundle.format_pattern(pattern, args, &mut errors);
            if !errors.is_empty() {
                return Err(Error::Format {
                    id: id.into(),
                    details: format!("{errors:?}"),
                });
            }
            return Ok(output);
        }
        if let Some(fallback) = &self.fallback {
            return fallback.format(id, args);
        }
        Err(Error::Missing { id: id.into() })
    }
    pub fn number(&self, value: impl Into<icu_decimal::input::Decimal>) -> String {
        self.number.format_to_string(&value.into())
    }
    pub fn date(&self, date: CalendarDate) -> String {
        self.date
            .format(&icu_date(date))
            .write_to_string()
            .into_owned()
    }
    /// Runtime errors from no-argument widget messages. Missing keys use the
    /// widget's documented English fallback; malformed messages stay observable.
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut *self.errors.lock().expect("localization errors lock"))
    }
}
fn icu_date(date: CalendarDate) -> icu_calendar::Date<Gregorian> {
    icu_calendar::Date::try_new_gregorian(i32::from(date.year()), date.month(), date.day())
        .expect("CalendarDate is validated")
}
impl Localizer for Catalog {
    fn locale(&self) -> &str {
        &self.locale
    }
    fn text(&self, id: &str, fallback: &'static str) -> Cow<'_, str> {
        match self.format(id, None) {
            Ok(text) => text,
            Err(Error::Missing { .. }) => Cow::Borrowed(fallback),
            Err(error) => {
                let message = error.to_string();
                let mut errors = self.errors.lock().expect("localization errors lock");
                if errors.len() < 64 && !errors.contains(&message) {
                    errors.push(message);
                }
                Cow::Borrowed(fallback)
            }
        }
    }
    fn month_year(&self, date: CalendarDate) -> String {
        self.month
            .format(&icu_date(date))
            .write_to_string()
            .into_owned()
    }
    fn weekday(&self, sunday_index: usize) -> Cow<'_, str> {
        Cow::Borrowed(&self.weekdays[sunday_index % 7])
    }
}
