//! Inline calculator for the root search: arithmetic ("2^10", "(3+4)*2")
//! and offline unit conversions ("40cm to in", "100 f in c", "3 GiB as MB").
//!
//! Windows-free and host-testable. Currencies are out of scope: their rates
//! need the network. A query only produces an answer when it is clearly a
//! calculation (an operator or a conversion), so app names such as "7-zip"
//! or a bare "2048" keep searching normally.

use std::f64::consts::PI;

/// Queries longer than this are never evaluated (bounds parser recursion).
const MAX_QUERY_CHARS: usize = 200;
/// Parenthesis nesting limit.
const MAX_DEPTH: usize = 32;

/// A calculator result row.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    /// The input as understood, e.g. "40 cm".
    pub input: String,
    /// The result, e.g. "15.7480315 in"; this is what Enter pastes.
    pub result: String,
    /// Captions under each side, e.g. "Centimeters" and "Inches".
    pub input_label: &'static str,
    pub result_label: &'static str,
}

/// Evaluate `query` as a conversion or an arithmetic expression.
pub fn evaluate(query: &str) -> Option<Answer> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return None;
    }
    convert(query).or_else(|| arithmetic(query))
}

fn arithmetic(query: &str) -> Option<Answer> {
    let (value, had_operator) = eval_expr(query)?;
    if !had_operator {
        return None;
    }
    Some(Answer {
        input: query.to_owned(),
        result: format_number(value)?,
        input_label: "Expression",
        result_label: "Result",
    })
}

/// `<expr> <unit> (to|in|as|->|→) <unit>`.
fn convert(query: &str) -> Option<Answer> {
    let lower = query.to_lowercase();
    // " in " is also the inch, so it is tried last and split at its last
    // occurrence: "40 in in cm" is inches to centimeters.
    for sep in [" to ", "->", "→", " as ", " in "] {
        let Some(at) = lower.rfind(sep) else {
            continue;
        };
        let (lhs, rhs) = (&lower[..at], lower[at + sep.len()..].trim());
        let Some(to) = unit_named(rhs) else {
            continue;
        };
        let Some((value, from)) = quantity(lhs) else {
            continue;
        };
        if from.dim != to.dim {
            continue;
        }
        let converted = to.of_base(from.to_base(value));
        return Some(Answer {
            input: format!("{} {}", format_number(value)?, from.symbol),
            result: format!("{} {}", format_number(converted)?, to.symbol),
            input_label: from.name,
            result_label: to.name,
        });
    }
    None
}

/// An expression followed by a unit, e.g. "40cm" or "(3 + 4) km".
fn quantity(text: &str) -> Option<(f64, &'static Unit)> {
    let text = text.trim();
    // Longest alias first, so "km" is not read as "k" + "m" and "°c" is
    // not read as "°".
    let (alias, unit) = UNITS
        .iter()
        .flat_map(|u| u.aliases.iter().map(move |a| (*a, u)))
        .filter(|(alias, _)| text.ends_with(alias))
        .filter(|(alias, _)| {
            let before = text[..text.len() - alias.len()].chars().next_back();
            before.is_some_and(|c| !c.is_alphabetic())
        })
        .max_by_key(|(alias, _)| alias.len())?;
    let (value, _) = eval_expr(&text[..text.len() - alias.len()])?;
    Some((value, unit))
}

fn unit_named(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.aliases.contains(&name))
}

/// Up to 10 significant digits, trailing zeros trimmed; scientific notation
/// for very large or small magnitudes. `None` for NaN and infinities.
fn format_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some("0".into());
    }
    let magnitude = value.abs().log10().floor() as i32;
    if !(-6..15).contains(&magnitude) {
        let text = format!("{value:.6e}");
        let (mantissa, exponent) = text.split_once('e')?;
        return Some(format!("{}e{exponent}", trim_zeros(mantissa)));
    }
    let decimals = (9 - magnitude).clamp(0, 10) as usize;
    let text = trim_zeros(&format!("{value:.decimals$}")).to_owned();
    Some(if text == "-0" { "0".into() } else { text })
}

fn trim_zeros(text: &str) -> &str {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        text
    }
}

// ---- expressions ----------------------------------------------------------------

/// Evaluate a whole arithmetic expression. Returns the value and whether it
/// used a binary operator (a lone number is not a calculation).
fn eval_expr(text: &str) -> Option<(f64, bool)> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
        depth: 0,
        had_operator: false,
    };
    let value = parser.sum()?;
    parser.skip_space();
    (parser.pos == parser.chars.len() && value.is_finite()).then_some((value, parser.had_operator))
}

/// Every intermediate must be finite: `1/(1/0)` would otherwise cancel an
/// infinity back to a plausible-looking `0`.
fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
    had_operator: bool,
}

impl Parser {
    fn skip_space(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_space();
        self.chars.get(self.pos).copied()
    }

    /// sum := product (('+' | '-') product)*
    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.pos += 1;
            self.had_operator = true;
            let rhs = self.product()?;
            value = finite(if op == '+' { value + rhs } else { value - rhs })?;
        }
        Some(value)
    }

    /// product := unary (('*' | '×' | '·' | '/' | '÷') unary)*
    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        while let Some(op @ ('*' | '×' | '·' | '/' | '÷')) = self.peek() {
            self.pos += 1;
            self.had_operator = true;
            let rhs = self.unary()?;
            value = finite(if matches!(op, '/' | '÷') {
                value / rhs
            } else {
                value * rhs
            })?;
        }
        Some(value)
    }

    /// unary := ('-' | '+') unary | power
    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some('-') => {
                self.pos += 1;
                self.unary().map(|v| -v)
            }
            Some('+') => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    /// power := atom ('^' unary)?, right-associative.
    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if self.peek() == Some('^') {
            self.pos += 1;
            self.had_operator = true;
            let exponent = self.unary()?;
            return finite(base.powf(exponent));
        }
        Some(base)
    }

    /// atom := number | '(' sum ')' | 'pi' | 'π'
    fn atom(&mut self) -> Option<f64> {
        match self.peek()? {
            '(' => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return None;
                }
                self.pos += 1;
                let value = self.sum()?;
                if self.peek() != Some(')') {
                    return None;
                }
                self.pos += 1;
                self.depth -= 1;
                Some(value)
            }
            'π' => {
                self.pos += 1;
                Some(PI)
            }
            'p' | 'P' if matches!(self.chars.get(self.pos + 1), Some('i' | 'I')) => {
                self.pos += 2;
                Some(PI)
            }
            c if c.is_ascii_digit() || c == '.' => self.number(),
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.pos;
        while self
            .chars
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_digit() || *c == '.')
        {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse().ok()
    }
}

// ---- units ----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    Length,
    Mass,
    Volume,
    Temperature,
    Time,
    Data,
    Area,
    Speed,
    Angle,
}

#[derive(Clone, Copy, Debug)]
enum Scale {
    /// Base units per one of this unit.
    Factor(f64),
    Celsius,
    Fahrenheit,
}

#[derive(Debug)]
struct Unit {
    /// Shown in answers.
    symbol: &'static str,
    /// Caption under an answer, e.g. "Centimeters".
    name: &'static str,
    /// Lowercase names accepted in queries.
    aliases: &'static [&'static str],
    dim: Dim,
    scale: Scale,
}

impl Unit {
    fn to_base(&self, value: f64) -> f64 {
        match self.scale {
            Scale::Factor(f) => value * f,
            Scale::Celsius => value + 273.15,
            Scale::Fahrenheit => (value - 32.0) * 5.0 / 9.0 + 273.15,
        }
    }

    /// A base-unit value expressed in this unit.
    fn of_base(&self, value: f64) -> f64 {
        match self.scale {
            Scale::Factor(f) => value / f,
            Scale::Celsius => value - 273.15,
            Scale::Fahrenheit => (value - 273.15) * 9.0 / 5.0 + 32.0,
        }
    }
}

const fn unit(
    symbol: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    dim: Dim,
    factor: f64,
) -> Unit {
    Unit {
        symbol,
        name,
        aliases,
        dim,
        scale: Scale::Factor(factor),
    }
}

/// Base units: meter, kilogram, liter, kelvin, second, byte, square meter,
/// meter per second, radian. Volumes are US customary.
static UNITS: &[Unit] = &[
    // Length
    unit(
        "µm",
        "Micrometers",
        &["µm", "um", "micrometer", "micrometers", "micron", "microns"],
        Dim::Length,
        1e-6,
    ),
    unit(
        "mm",
        "Millimeters",
        &[
            "mm",
            "millimeter",
            "millimeters",
            "millimetre",
            "millimetres",
        ],
        Dim::Length,
        1e-3,
    ),
    unit(
        "cm",
        "Centimeters",
        &[
            "cm",
            "centimeter",
            "centimeters",
            "centimetre",
            "centimetres",
        ],
        Dim::Length,
        1e-2,
    ),
    unit(
        "m",
        "Meters",
        &["m", "meter", "meters", "metre", "metres"],
        Dim::Length,
        1.0,
    ),
    unit(
        "km",
        "Kilometers",
        &["km", "kilometer", "kilometers", "kilometre", "kilometres"],
        Dim::Length,
        1e3,
    ),
    unit(
        "in",
        "Inches",
        &["in", "inch", "inches", "\""],
        Dim::Length,
        0.0254,
    ),
    unit(
        "ft",
        "Feet",
        &["ft", "foot", "feet", "'"],
        Dim::Length,
        0.3048,
    ),
    unit("yd", "Yards", &["yd", "yard", "yards"], Dim::Length, 0.9144),
    unit(
        "mi",
        "Miles",
        &["mi", "mile", "miles"],
        Dim::Length,
        1609.344,
    ),
    unit(
        "nmi",
        "Nautical Miles",
        &["nmi", "nautical mile", "nautical miles"],
        Dim::Length,
        1852.0,
    ),
    // Mass
    unit(
        "mg",
        "Milligrams",
        &["mg", "milligram", "milligrams"],
        Dim::Mass,
        1e-6,
    ),
    unit("g", "Grams", &["g", "gram", "grams"], Dim::Mass, 1e-3),
    unit(
        "kg",
        "Kilograms",
        &["kg", "kilogram", "kilograms", "kilo", "kilos"],
        Dim::Mass,
        1.0,
    ),
    unit(
        "t",
        "Tonnes",
        &["t", "tonne", "tonnes", "metric ton", "metric tons"],
        Dim::Mass,
        1e3,
    ),
    unit(
        "oz",
        "Ounces",
        &["oz", "ounce", "ounces"],
        Dim::Mass,
        0.028349523125,
    ),
    unit(
        "lb",
        "Pounds",
        &["lb", "lbs", "pound", "pounds"],
        Dim::Mass,
        0.45359237,
    ),
    unit(
        "st",
        "Stone",
        &["st", "stone", "stones"],
        Dim::Mass,
        6.35029318,
    ),
    // Volume (US customary)
    unit(
        "ml",
        "Milliliters",
        &[
            "ml",
            "milliliter",
            "milliliters",
            "millilitre",
            "millilitres",
        ],
        Dim::Volume,
        1e-3,
    ),
    unit(
        "cl",
        "Centiliters",
        &["cl", "centiliter", "centiliters"],
        Dim::Volume,
        1e-2,
    ),
    unit(
        "dl",
        "Deciliters",
        &["dl", "deciliter", "deciliters"],
        Dim::Volume,
        1e-1,
    ),
    unit(
        "l",
        "Liters",
        &["l", "liter", "liters", "litre", "litres"],
        Dim::Volume,
        1.0,
    ),
    unit(
        "m³",
        "Cubic Meters",
        &["m3", "m³", "cubic meter", "cubic meters"],
        Dim::Volume,
        1e3,
    ),
    unit(
        "tsp",
        "Teaspoons",
        &["tsp", "teaspoon", "teaspoons"],
        Dim::Volume,
        0.00492892159375,
    ),
    unit(
        "tbsp",
        "Tablespoons",
        &["tbsp", "tablespoon", "tablespoons"],
        Dim::Volume,
        0.01478676478125,
    ),
    unit(
        "fl oz",
        "Fluid Ounces",
        &["fl oz", "floz", "fluid ounce", "fluid ounces"],
        Dim::Volume,
        0.0295735295625,
    ),
    unit("cup", "Cups", &["cup", "cups"], Dim::Volume, 0.2365882365),
    unit(
        "pt",
        "Pints",
        &["pt", "pint", "pints"],
        Dim::Volume,
        0.473176473,
    ),
    unit(
        "qt",
        "Quarts",
        &["qt", "quart", "quarts"],
        Dim::Volume,
        0.946352946,
    ),
    unit(
        "gal",
        "Gallons",
        &["gal", "gallon", "gallons"],
        Dim::Volume,
        3.785411784,
    ),
    // Temperature
    Unit {
        symbol: "°C",
        name: "Celsius",
        aliases: &["c", "°c", "celsius", "degc"],
        dim: Dim::Temperature,
        scale: Scale::Celsius,
    },
    Unit {
        symbol: "°F",
        name: "Fahrenheit",
        aliases: &["f", "°f", "fahrenheit", "degf"],
        dim: Dim::Temperature,
        scale: Scale::Fahrenheit,
    },
    unit("K", "Kelvin", &["k", "kelvin"], Dim::Temperature, 1.0),
    // Time (a year is 365.25 days)
    unit(
        "ms",
        "Milliseconds",
        &["ms", "millisecond", "milliseconds"],
        Dim::Time,
        1e-3,
    ),
    unit(
        "s",
        "Seconds",
        &["s", "sec", "secs", "second", "seconds"],
        Dim::Time,
        1.0,
    ),
    unit(
        "min",
        "Minutes",
        &["min", "mins", "minute", "minutes"],
        Dim::Time,
        60.0,
    ),
    unit(
        "h",
        "Hours",
        &["h", "hr", "hrs", "hour", "hours"],
        Dim::Time,
        3600.0,
    ),
    unit("d", "Days", &["d", "day", "days"], Dim::Time, 86_400.0),
    unit(
        "wk",
        "Weeks",
        &["wk", "week", "weeks"],
        Dim::Time,
        604_800.0,
    ),
    unit(
        "yr",
        "Years",
        &["yr", "year", "years"],
        Dim::Time,
        31_557_600.0,
    ),
    // Data (kB/MB/GB/TB are decimal; KiB/MiB/GiB/TiB binary)
    unit("bit", "Bits", &["bit", "bits"], Dim::Data, 0.125),
    unit("B", "Bytes", &["b", "byte", "bytes"], Dim::Data, 1.0),
    unit(
        "kB",
        "Kilobytes",
        &["kb", "kilobyte", "kilobytes"],
        Dim::Data,
        1e3,
    ),
    unit(
        "MB",
        "Megabytes",
        &["mb", "megabyte", "megabytes"],
        Dim::Data,
        1e6,
    ),
    unit(
        "GB",
        "Gigabytes",
        &["gb", "gigabyte", "gigabytes"],
        Dim::Data,
        1e9,
    ),
    unit(
        "TB",
        "Terabytes",
        &["tb", "terabyte", "terabytes"],
        Dim::Data,
        1e12,
    ),
    unit(
        "KiB",
        "Kibibytes",
        &["kib", "kibibyte", "kibibytes"],
        Dim::Data,
        1024.0,
    ),
    unit(
        "MiB",
        "Mebibytes",
        &["mib", "mebibyte", "mebibytes"],
        Dim::Data,
        1_048_576.0,
    ),
    unit(
        "GiB",
        "Gibibytes",
        &["gib", "gibibyte", "gibibytes"],
        Dim::Data,
        1_073_741_824.0,
    ),
    unit(
        "TiB",
        "Tebibytes",
        &["tib", "tebibyte", "tebibytes"],
        Dim::Data,
        1_099_511_627_776.0,
    ),
    // Area
    unit(
        "cm²",
        "Square Centimeters",
        &["cm2", "cm²", "sq cm"],
        Dim::Area,
        1e-4,
    ),
    unit(
        "m²",
        "Square Meters",
        &["m2", "m²", "sq m", "square meter", "square meters"],
        Dim::Area,
        1.0,
    ),
    unit(
        "km²",
        "Square Kilometers",
        &["km2", "km²", "sq km"],
        Dim::Area,
        1e6,
    ),
    unit(
        "in²",
        "Square Inches",
        &["in2", "in²", "sq in"],
        Dim::Area,
        0.00064516,
    ),
    unit(
        "ft²",
        "Square Feet",
        &["ft2", "ft²", "sq ft", "square feet"],
        Dim::Area,
        0.09290304,
    ),
    unit(
        "yd²",
        "Square Yards",
        &["yd2", "yd²", "sq yd"],
        Dim::Area,
        0.83612736,
    ),
    unit(
        "ac",
        "Acres",
        &["ac", "acre", "acres"],
        Dim::Area,
        4046.8564224,
    ),
    unit(
        "ha",
        "Hectares",
        &["ha", "hectare", "hectares"],
        Dim::Area,
        1e4,
    ),
    unit(
        "mi²",
        "Square Miles",
        &["mi2", "mi²", "sq mi"],
        Dim::Area,
        2_589_988.110336,
    ),
    // Speed
    unit("m/s", "Meters per Second", &["m/s", "mps"], Dim::Speed, 1.0),
    unit(
        "km/h",
        "Kilometers per Hour",
        &["km/h", "kmh", "kph"],
        Dim::Speed,
        1.0 / 3.6,
    ),
    unit("mph", "Miles per Hour", &["mph"], Dim::Speed, 0.44704),
    unit(
        "kn",
        "Knots",
        &["kn", "knot", "knots"],
        Dim::Speed,
        1852.0 / 3600.0,
    ),
    unit(
        "ft/s",
        "Feet per Second",
        &["ft/s", "fps"],
        Dim::Speed,
        0.3048,
    ),
    // Angle
    unit(
        "°",
        "Degrees",
        &["°", "deg", "degree", "degrees"],
        Dim::Angle,
        PI / 180.0,
    ),
    unit(
        "rad",
        "Radians",
        &["rad", "radian", "radians"],
        Dim::Angle,
        1.0,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn result(query: &str) -> Option<String> {
        evaluate(query).map(|a| a.result)
    }

    #[test]
    fn converts_lengths_in_many_spellings() {
        assert_eq!(result("40cm to in").as_deref(), Some("15.7480315 in"));
        assert_eq!(result("40 cm in inches").as_deref(), Some("15.7480315 in"));
        assert_eq!(result("40 CM AS IN").as_deref(), Some("15.7480315 in"));
        assert_eq!(result("1 mi -> km").as_deref(), Some("1.609344 km"));
        assert_eq!(result("6 ft → m").as_deref(), Some("1.8288 m"));
        let answer = evaluate("40cm to in").unwrap();
        assert_eq!(answer.input, "40 cm");
        assert_eq!(
            (answer.input_label, answer.result_label),
            ("Centimeters", "Inches")
        );
    }

    #[test]
    fn inch_and_the_in_keyword_coexist() {
        assert_eq!(result("10 in in cm").as_deref(), Some("25.4 cm"));
        assert_eq!(result("10 in to cm").as_deref(), Some("25.4 cm"));
        assert_eq!(result("10 inches in cm").as_deref(), Some("25.4 cm"));
    }

    #[test]
    fn converts_temperatures() {
        assert_eq!(result("100f to c").as_deref(), Some("37.77777778 °C"));
        assert_eq!(result("0 °C in F").as_deref(), Some("32 °F"));
        assert_eq!(result("-40 c to f").as_deref(), Some("-40 °F"));
        assert_eq!(result("0 k to c").as_deref(), Some("-273.15 °C"));
    }

    #[test]
    fn converts_other_dimensions() {
        assert_eq!(result("1 kg to lb").as_deref(), Some("2.204622622 lb"));
        assert_eq!(result("1 gal to l").as_deref(), Some("3.785411784 l"));
        assert_eq!(result("1 GiB to MB").as_deref(), Some("1073.741824 MB"));
        assert_eq!(result("90 min in h").as_deref(), Some("1.5 h"));
        assert_eq!(result("100 kph to mph").as_deref(), Some("62.13711922 mph"));
        assert_eq!(result("1 acre to m2").as_deref(), Some("4046.856422 m²"));
        assert_eq!(result("180 deg to rad").as_deref(), Some("3.141592654 rad"));
    }

    #[test]
    fn conversions_accept_expressions() {
        assert_eq!(
            result("(3 + 4) km to mi").as_deref(),
            Some("4.349598346 mi")
        );
        assert_eq!(result("2*50cm to m").as_deref(), Some("1 m"));
    }

    #[test]
    fn mismatched_or_unknown_units_give_nothing() {
        assert_eq!(result("5 kg to m"), None);
        assert_eq!(result("5 parsecs to m"), None);
        assert_eq!(result("40cm"), None);
        assert_eq!(result("how to"), None);
    }

    #[test]
    fn evaluates_arithmetic() {
        assert_eq!(result("2+2").as_deref(), Some("4"));
        assert_eq!(result("2 ^ 10").as_deref(), Some("1024"));
        assert_eq!(result("2^3^2").as_deref(), Some("512"));
        assert_eq!(result("(1 + 2) * 3 - 4 / 8").as_deref(), Some("8.5"));
        assert_eq!(result("-3 * -3").as_deref(), Some("9"));
        assert_eq!(result("10 ÷ 4").as_deref(), Some("2.5"));
        assert_eq!(result("2 * pi").as_deref(), Some("6.283185307"));
        assert_eq!(result("0.1 + 0.2").as_deref(), Some("0.3"));
        assert_eq!(result("1e3"), None);
    }

    #[test]
    fn plain_searches_are_not_calculations() {
        for query in [
            "7-zip",
            "2048",
            "-5",
            "(5)",
            "notepad",
            "1password",
            "",
            "1/0",
            "1/(1/0)",
            "1/(2^1024)",
        ] {
            assert_eq!(result(query), None, "{query}");
        }
    }

    #[test]
    fn formats_large_and_small_numbers() {
        assert_eq!(result("10^20").as_deref(), Some("1e20"));
        assert_eq!(result("1/3000000000").as_deref(), Some("3.333333e-10"));
        assert_eq!(result("1/3").as_deref(), Some("0.3333333333"));
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let deep = format!("{}1{}", "(".repeat(40), ")".repeat(40));
        assert_eq!(result(&format!("{deep}+1")), None);
    }
}
