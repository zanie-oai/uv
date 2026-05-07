use std::fmt;

use jiff::Timestamp;
use owo_colors::OwoColorize;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

/// The style of a uv logging line.
pub struct UvFormat {
    pub display_timestamp: bool,
    pub display_level: bool,
    pub show_spans: bool,
}

impl Default for UvFormat {
    /// Regardless of the tracing level, show messages without any adornment.
    fn default() -> Self {
        Self {
            display_timestamp: false,
            display_level: true,
            show_spans: false,
        }
    }
}

/// See <https://docs.rs/tracing-subscriber/0.3.18/src/tracing_subscriber/fmt/format/mod.rs.html#1026-1156>
impl<S, N> FormatEvent<S, N> for UvFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        if self.display_timestamp {
            if ansi {
                write!(writer, "{} ", Timestamp::now().dimmed())?;
            } else {
                write!(writer, "{} ", Timestamp::now())?;
            }
        }

        if self.display_level {
            let level = meta.level();
            // Same colors as tracing
            if ansi {
                match *level {
                    tracing::Level::TRACE => write!(writer, "{} ", level.purple())?,
                    tracing::Level::DEBUG => write!(writer, "{} ", level.blue())?,
                    tracing::Level::INFO => write!(writer, "{} ", level.green())?,
                    tracing::Level::WARN => write!(writer, "{} ", level.yellow())?,
                    tracing::Level::ERROR => write!(writer, "{} ", level.red())?,
                }
            } else {
                write!(writer, "{level} ")?;
            }
        }

        if self.show_spans {
            let span = event.parent();
            let mut seen = false;

            let span = span
                .and_then(|id| ctx.span(id))
                .or_else(|| ctx.lookup_current());

            let scope = span.into_iter().flat_map(|span| span.scope().from_root());

            for span in scope {
                seen = true;
                if ansi {
                    write!(writer, "{}:", span.metadata().name().bold())?;
                } else {
                    write!(writer, "{}:", span.metadata().name())?;
                }
            }

            if seen {
                writer.write_char(' ')?;
            }
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

/// Field formatter for uv logging.
///
/// The event formatter is responsible for uv's own log colors, such as the level prefix. Field
/// values can come from arbitrary `Display` or `Debug` implementations, so strip any ANSI escape
/// sequences there before writing them to the log line.
#[derive(Debug, Default, Clone, Copy)]
pub struct UvFields;

impl<'writer> FormatFields<'writer> for UvFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = UvFieldsVisitor {
            writer,
            is_empty: true,
            result: Ok(()),
        };
        fields.record(&mut visitor);
        visitor.result
    }

    fn add_fields(
        &self,
        current: &'writer mut FormattedFields<Self>,
        fields: &tracing::span::Record<'_>,
    ) -> fmt::Result {
        if !current.fields.is_empty() {
            current.fields.push(' ');
        }
        self.format_fields(current.as_writer(), fields)
    }
}

struct UvFieldsVisitor<'writer> {
    writer: Writer<'writer>,
    is_empty: bool,
    result: fmt::Result,
}

impl UvFieldsVisitor<'_> {
    fn record_value(&mut self, field: &Field, value: impl fmt::Display) {
        if self.result.is_err() {
            return;
        }

        if self.is_empty {
            self.is_empty = false;
        } else {
            self.result = write!(self.writer, " ");
            if self.result.is_err() {
                return;
            }
        }

        let value = value.to_string();
        let value = anstream::adapter::strip_str(&value);
        self.result = match field.name() {
            "message" => write!(self.writer, "{value}"),
            name if name.starts_with("r#") => {
                write!(self.writer, "{}={value}", &name[2..])
            }
            name => write!(self.writer, "{name}={value}"),
        };
    }
}

impl Visit for UvFieldsVisitor<'_> {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_value(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.record_value(field, value);
        } else {
            self.record_value(field, format!("{value:?}"));
        }
    }

    fn record_error(&mut self, field: &Field, error: &(dyn std::error::Error + 'static)) {
        let mut value = error.to_string();

        if let Some(first_source) = error.source() {
            value.push(' ');
            value.push_str(field.name());
            value.push_str(".sources=[");

            let mut source = Some(first_source);
            let mut first = true;
            while let Some(error) = source {
                if first {
                    first = false;
                } else {
                    value.push_str(", ");
                }
                value.push_str(&error.to_string());
                source = error.source();
            }

            value.push(']');
        }

        self.record_value(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }
}
