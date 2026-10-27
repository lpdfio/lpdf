use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub space: [f32; 6],
    pub border: [f32; 6],
    pub radius: [f32; 6],
    pub width: [f32; 6],
    pub height: [f32; 6],
    pub grid_col: [f32; 6],
    pub text_size: [f32; 6],
    /// Factors of the font size, not pt.
    pub line_height: [f32; 6],
    /// Names for the slots of the scale, each with the index of its slot: `caption` for `xs` is `("caption", 0)`.
    /// An alias is shared by every row, so it names the same slot wherever a scale value is accepted.
    pub aliases: HashMap<String, usize>,
    pub colors: HashMap<String, String>,
    pub gradients: HashMap<String, Gradient>,
}

/// Names an alias cannot take, because an attribute that takes a scale value has them as a keyword of its own:
/// `width="fill"`, and `height="full"` or `"fill"`, which the height row shares the attribute with.
const RESERVED_ALIASES: [&str; 2] = ["fill", "full"];

/// What a fill starts with when it names a gradient of the tokens: `gradient:brand-fade`.
pub const GRADIENT_PREFIX: &str = "gradient:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
}

/// One color of a gradient, at a position from 0 (where it starts) to 1 (where it ends).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    /// A normalised hex color.
    pub color: String,
}

/// A gradient of the tokens, resolved: every color is hex and the stops are in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    /// In degrees: 0 runs top to bottom and 90 left to right. A radial gradient has no direction.
    pub angle: f32,
    /// At least two, by increasing offset, the first at 0 and the last at 1.
    pub stops: Vec<GradientStop>,
}

/// A gradient as the document writes it, before its type, angle and colors are checked.
pub struct GradientSpec<'a> {
    pub kind: &'a str,
    pub angle: Option<&'a str>,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    /// `(offset, color)` pairs. When there are any, `from` and `to` are not used.
    pub stops: Vec<(&'a str, &'a str)>,
}

/// The name of a gradient or an alias, the way the schema's `TokenName` type spells it: the `Fill` type spells a
/// gradient's after `gradient:`.
fn is_token_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Parse a stop offset like "60%" into a position from 0 to 1.
fn parse_stop_offset(val: &str) -> Option<f32> {
    let percent: f32 = val.trim().strip_suffix('%')?.trim().parse().ok()?;
    (0.0..=100.0).contains(&percent).then_some(percent / 100.0)
}

/// A font definition resolved from `<assets><fonts>`.
#[derive(Debug, Clone, PartialEq)]
pub enum FontDef {
    /// One of the 14 PDF core (built-in Type 1) fonts. No bytes needed.
    Core(String),
    /// A registry key registered via `loadFont(key, bytes)`. Bytes must be
    /// supplied in the WASI envelope's `fonts` field at render time.
    Ref(String),
}

/// Per-glyph advance widths for a custom font, in 1/1000 em units.
/// Read from the font's own bytes when it is loaded, or supplied by the adapter, so the
/// layout engine and the renderer measure custom-font text with the same advances.
#[derive(Debug, Clone, Default)]
pub struct FontWidths {
    /// Advance width for a character the table does not list: the font's glyph 0
    /// (.notdef), which is what the encoder writes for it.
    pub default: u16,
    /// Advances for code points 32 (space) through 126 (~), normalised to 1000 UPM.
    /// Empty, or exactly 95 elements with index = codepoint - 32. Only tables supplied
    /// as JSON use it; a table read from a font lists every character in `advances`.
    pub ascii: Vec<u16>,
    /// Advance of every character the font maps, normalised to 1000 UPM. Shared, so a
    /// table is cloned into each layout call without copying the map.
    pub advances: Rc<HashMap<char, u16>>,
}

impl FontWidths {
    /// Advance of `c` in 1/1000 em: its own, else the `ascii` entry, else `default`.
    pub fn advance(&self, c: char) -> u16 {
        if let Some(&advance) = self.advances.get(&c) {
            return advance;
        }
        let cp = c as u32;
        if (32..=126).contains(&cp) {
            if let Some(&advance) = self.ascii.get((cp - 32) as usize) {
                return advance;
            }
        }
        self.default
    }

    /// Width of `text` at `size` pt.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.advance(c) as f32 * size / 1000.0).sum()
    }
}

pub const SCALE_NAMES: [&str; 6] = ["xs", "s", "m", "l", "xl", "xxl"];

pub fn scale_idx(name: &str) -> Option<usize> {
    SCALE_NAMES.iter().position(|&n| n == name)
}

/// Parse a pt value like "12pt", "0.5pt", or bare "0".
pub fn parse_pt(val: &str) -> Option<f32> {
    let s = val.trim();
    if s == "0" { return Some(0.0); }
    s.strip_suffix("pt")?.trim().parse().ok()
}

/// Parse a factor above 0, like "1.5", the way the schema's FactorValue does: a bare number, no unit.
pub fn parse_factor(val: &str) -> Option<f32> {
    val.trim().parse::<f32>().ok().filter(|f| f.is_finite() && *f > 0.0)
}

/// Parse an opacity, the way the schema's OpacityValue does: a number from 0 (invisible) to 1 (opaque).
pub fn parse_opacity(val: &str) -> Result<f32, String> {
    val.trim()
        .parse::<f32>()
        .ok()
        .filter(|f| (0.0..=1.0).contains(f))
        .ok_or_else(|| format!("invalid opacity '{val}': expected a number from 0 to 1"))
}

pub fn normalize_hex(s: &str) -> Result<String, String> {
    let h = match s.strip_prefix('#') {
        Some(h) => h,
        None => return Err(format!("color must start with '#': {s}")),
    };
    match h.len() {
        3 => {
            let chars: Vec<char> = h.chars().collect();
            Ok(format!(
                "#{0}{0}{1}{1}{2}{2}",
                chars[0].to_lowercase(),
                chars[1].to_lowercase(),
                chars[2].to_lowercase()
            ))
        }
        6 => Ok(format!("#{}", h.to_lowercase())),
        _ => Err(format!("invalid hex color: {s}")),
    }
}

impl Tokens {
    /// The slot a scale value names: one of the six (`xs`), or an alias of the document (`caption`).
    fn slot(&self, val: &str) -> Option<usize> {
        scale_idx(val).or_else(|| self.aliases.get(val).copied())
    }

    /// Name a slot of the scale, as the document's `<alias name value/>` does. The value must be one of the six
    /// slots: an alias names a slot, not another alias, so it never has to be followed.
    pub fn add_alias(&mut self, name: &str, value: &str) -> Result<(), String> {
        let fail = |reason: String| format!("alias '{name}': {reason}");
        if !is_token_name(name) {
            return Err(fail("the name must be lowercase letters, digits and hyphens, starting with a letter".into()));
        }
        if scale_idx(name).is_some() {
            return Err(fail("xs, s, m, l, xl and xxl are the slots themselves, and cannot name another".into()));
        }
        if RESERVED_ALIASES.contains(&name) {
            return Err(fail(format!("'{name}' is a keyword of the attributes that take a scale value")));
        }
        let slot = scale_idx(value)
            .ok_or_else(|| fail(format!("invalid value '{value}': expected a slot from xs to xxl")))?;
        if self.aliases.contains_key(name) {
            return Err(fail("it is defined twice".into()));
        }
        self.aliases.insert(name.to_string(), slot);
        Ok(())
    }

    /// What names a slot, for the error that says a value is none of them: the six, and the document's aliases.
    pub(crate) fn slot_names(&self) -> String {
        if self.aliases.is_empty() {
            return "a slot from xs to xxl".to_string();
        }
        let mut aliases: Vec<&str> = self.aliases.keys().map(String::as_str).collect();
        aliases.sort_unstable();
        format!("a slot from xs to xxl, an alias ({})", aliases.join(", "))
    }

    /// A slot of the scale, an alias of one, or a pt value: the `name` row's value for it.
    fn resolve_scale(&self, val: &str, scale: &[f32; 6], name: &str) -> Result<f32, String> {
        if let Some(i) = self.slot(val) {
            return Ok(scale[i]);
        }
        if let Some(v) = parse_pt(val) {
            return Ok(v);
        }
        Err(format!("invalid {name} value: '{val}': expected {} or a pt value", self.slot_names()))
    }

    pub fn resolve_space(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.space, "space")
    }

    pub fn resolve_border_thickness(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.border, "border")
    }

    pub fn resolve_radius(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.radius, "radius")
    }

    /// Width token OR explicit pt value
    pub fn resolve_width(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.width, "width")
    }

    /// Height token OR explicit pt value. `full` and `fill` are not read here: the boxes that have them read them first.
    pub fn resolve_height(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.height, "height")
    }

    /// Grid column min-width: scale token or pt value
    pub fn resolve_grid_col(&self, val: &str) -> Result<f32, String> {
        self.resolve_scale(val, &self.grid_col, "grid")
    }

    /// Text size: scale token, pt value, or plain number
    pub fn resolve_text_size(&self, val: &str) -> Result<f32, String> {
        if let Some(i) = self.slot(val) {
            return Ok(self.text_size[i]);
        }
        if let Some(v) = parse_pt(val) {
            return Ok(v);
        }
        val.parse::<f32>()
            .map_err(|_| format!("invalid text size: '{val}': expected {} or a pt value", self.slot_names()))
    }

    /// Line height: a token of the line-height row, or a factor of the font size above 0.
    pub fn resolve_line_height(&self, val: &str) -> Result<f32, String> {
        if let Some(i) = self.slot(val) {
            return Ok(self.line_height[i]);
        }
        parse_factor(val).ok_or_else(|| {
            format!("invalid line-height '{val}': expected {} or a factor above 0, like 1.5", self.slot_names())
        })
    }

    pub fn resolve_color(&self, val: &str) -> Result<String, String> {
        if val.starts_with('#') {
            return normalize_hex(val);
        }
        if val.starts_with(GRADIENT_PREFIX) {
            return Err(format!(
                "'{val}' is a gradient, which can fill a background or a shape but not color a text, a stroke or a border"
            ));
        }
        self.colors
            .get(val)
            .cloned()
            .ok_or_else(|| format!("unknown color token: '{val}'"))
    }

    /// A background or the fill of a shape: a color, or `gradient:name` for a gradient of the tokens, which is
    /// returned as written once it is known to exist.
    pub fn resolve_fill(&self, val: &str) -> Result<String, String> {
        match val.strip_prefix(GRADIENT_PREFIX) {
            Some(name) if self.gradients.contains_key(name) => Ok(val.to_string()),
            Some(name) => Err(format!("unknown gradient token: '{name}'")),
            None => self.resolve_color(val),
        }
    }

    /// Check a gradient the document defines and look up its colors.
    ///
    /// The colors come from the `colors` row, so call this once the row is complete.
    pub fn resolve_gradient(&self, name: &str, spec: &GradientSpec) -> Result<Gradient, String> {
        let fail = |reason: String| format!("gradient '{name}': {reason}");
        if !is_token_name(name) {
            return Err(fail("the name must be lowercase letters, digits and hyphens, starting with a letter".into()));
        }
        let kind = match spec.kind {
            "linear" => GradientKind::Linear,
            "radial" => GradientKind::Radial,
            other    => return Err(fail(format!("invalid type '{other}': expected linear or radial"))),
        };
        let angle = match spec.angle {
            None => 0.0,
            Some(val) => val
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|angle| angle.is_finite())
                .ok_or_else(|| fail(format!("invalid angle '{val}': expected degrees, like 90")))?,
        };

        let written: Vec<(&str, &str)> = if spec.stops.is_empty() {
            match (spec.from, spec.to) {
                (Some(from), Some(to)) => vec![("0%", from), ("100%", to)],
                _ => return Err(fail("give from and to, or at least two stops".into())),
            }
        } else {
            spec.stops.clone()
        };
        if written.len() < 2 {
            return Err(fail("a gradient needs at least two stops".into()));
        }

        let mut stops: Vec<GradientStop> = Vec::with_capacity(written.len() + 2);
        for (offset, color) in written {
            let offset = parse_stop_offset(offset)
                .ok_or_else(|| fail(format!("invalid stop offset '{offset}': expected a percentage from 0% to 100%")))?;
            let color = self.resolve_color(color).map_err(fail)?;
            if let Some(previous) = stops.last()
                && offset <= previous.offset
            {
                return Err(fail(format!(
                    "the stop offsets must increase, but {}% comes after {}%",
                    offset * 100.0,
                    previous.offset * 100.0
                )));
            }
            stops.push(GradientStop { offset, color });
        }

        // A gradient keeps its first color up to the first stop and its last color after the last, so the
        // writer can take the stops as covering 0 to 1.
        if let Some(first) = stops.first().filter(|stop| stop.offset > 0.0) {
            stops.insert(0, GradientStop { offset: 0.0, color: first.color.clone() });
        }
        if let Some(last) = stops.last().filter(|stop| stop.offset < 1.0) {
            stops.push(GradientStop { offset: 1.0, color: last.color.clone() });
        }
        Ok(Gradient { kind, angle, stops })
    }

    /// Parse "thickness color" border shorthand
    pub fn resolve_border(&self, val: &str) -> Result<(f32, String), String> {
        let space = val.find(' ').ok_or_else(|| {
            format!("border requires 'thickness color': '{val}'")
        })?;
        let thickness_str = &val[..space];
        let color_str = val[space + 1..].trim();
        let thickness = self.resolve_border_thickness(thickness_str)?;
        let color = self.resolve_color(color_str)?;
        Ok((thickness, color))
    }

    /// CSS shorthand spacing: 1–4 values, each a space token or pt value
    pub fn resolve_spacing(&self, val: &str) -> Result<[f32; 4], String> {
        let parts: Vec<&str> = val.split_whitespace().collect();
        match parts.len() {
            1 => {
                let v = self.resolve_space(parts[0])?;
                Ok([v, v, v, v])
            }
            2 => {
                let tb = self.resolve_space(parts[0])?;
                let rl = self.resolve_space(parts[1])?;
                Ok([tb, rl, tb, rl])
            }
            3 => {
                let t = self.resolve_space(parts[0])?;
                let rl = self.resolve_space(parts[1])?;
                let b = self.resolve_space(parts[2])?;
                Ok([t, rl, b, rl])
            }
            4 => Ok([
                self.resolve_space(parts[0])?,
                self.resolve_space(parts[1])?,
                self.resolve_space(parts[2])?,
                self.resolve_space(parts[3])?,
            ]),
            _ => Err(format!("invalid spacing shorthand: '{val}'")),
        }
    }
}

impl Default for Tokens {
    fn default() -> Self {
        let mut colors = HashMap::new();
        for (name, val) in [
            ("primary", "#1763cf"),
            ("secondary", "#6a40bf"),
            ("accent", "#f2800d"),
            ("success", "#248f4b"),
            ("warning", "#e09006"),
            ("danger", "#d92626"),
            ("info", "#1d8fc9"),
            ("surface", "#ffffff"),
            ("surface-alt", "#f5f5f5"),
            ("text", "#1a1a1a"),
            ("text-muted", "#737373"),
            ("line", "#737373"),
            ("line-muted", "#d4d4d4"),
        ] {
            colors.insert(name.to_string(), val.to_string());
        }
        Tokens {
            space: [2.0, 4.0, 8.0, 14.0, 20.0, 32.0],
            border: [0.5, 1.0, 1.5, 2.0, 3.0, 4.0],
            radius: [2.0, 4.0, 6.0, 10.0, 16.0, 24.0],
            width: [120.0, 200.0, 280.0, 360.0, 440.0, 515.0],
            height: [16.0, 24.0, 32.0, 48.0, 64.0, 96.0],
            grid_col: [60.0, 100.0, 140.0, 180.0, 220.0, 280.0],
            text_size: [7.0, 9.0, 11.0, 14.0, 20.0, 28.0],
            line_height: [1.0, 1.1, 1.2, 1.4, 1.6, 2.0],
            aliases: HashMap::new(),
            colors,
            gradients: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_space_token() {
        let t = Tokens::default();
        assert_eq!(t.resolve_space("m").unwrap(), 8.0);
        assert_eq!(t.resolve_space("xs").unwrap(), 2.0);
        assert_eq!(t.resolve_space("xxl").unwrap(), 32.0);
    }

    #[test]
    fn resolve_space_pt() {
        let t = Tokens::default();
        assert_eq!(t.resolve_space("12pt").unwrap(), 12.0);
        assert_eq!(t.resolve_space("0.5pt").unwrap(), 0.5);
    }

    #[test]
    fn resolve_line_height_token_and_factor() {
        let t = Tokens::default();
        assert_eq!(t.resolve_line_height("xs").unwrap(), 1.0);
        assert_eq!(t.resolve_line_height("m").unwrap(), 1.2);
        assert_eq!(t.resolve_line_height("xxl").unwrap(), 2.0);
        assert_eq!(t.resolve_line_height("1.5").unwrap(), 1.5);
        assert_eq!(t.resolve_line_height("2").unwrap(), 2.0);
    }

    #[test]
    fn resolve_line_height_follows_a_redefined_row() {
        let t = Tokens { line_height: [1.0, 1.25, 1.5, 1.75, 2.0, 2.5], ..Tokens::default() };
        assert_eq!(t.resolve_line_height("m").unwrap(), 1.5);
        assert_eq!(t.resolve_line_height("1.5").unwrap(), 1.5, "a number is not looked up");
    }

    #[test]
    fn resolve_line_height_rejects_what_is_not_a_factor_above_zero() {
        let t = Tokens::default();
        for bad in ["0", "-1", "1.5pt", "tall", "", "inf", "NaN", "1,5"] {
            let err = t.resolve_line_height(bad).unwrap_err();
            assert!(err.starts_with(&format!("invalid line-height '{bad}'")), "{bad}: {err}");
        }
    }

    #[test]
    fn resolve_height_token_and_pt() {
        let t = Tokens::default();
        assert_eq!(t.resolve_height("xs").unwrap(), 16.0);
        assert_eq!(t.resolve_height("m").unwrap(), 32.0);
        assert_eq!(t.resolve_height("xxl").unwrap(), 96.0);
        assert_eq!(t.resolve_height("28pt").unwrap(), 28.0);
    }

    #[test]
    fn resolve_height_follows_a_redefined_row_and_is_not_the_width_row() {
        let t = Tokens { height: [10.0, 20.0, 30.0, 40.0, 50.0, 60.0], ..Tokens::default() };
        assert_eq!(t.resolve_height("l").unwrap(), 40.0);
        assert_eq!(t.resolve_width("l").unwrap(), 360.0, "the width row is a row of its own");
    }

    #[test]
    fn resolve_height_rejects_what_is_neither_a_slot_nor_a_length() {
        let t = Tokens::default();
        assert_eq!(
            t.resolve_height("medum").unwrap_err(),
            "invalid height value: 'medum': expected a slot from xs to xxl or a pt value"
        );
    }

    // ── Aliases ───────────────────────────────────────────────────────────────

    fn with_aliases(aliases: &[(&str, &str)]) -> Tokens {
        let mut tokens = Tokens::default();
        for (name, value) in aliases {
            tokens.add_alias(name, value).unwrap();
        }
        tokens
    }

    #[test]
    fn an_alias_resolves_to_its_slot_in_every_scale() {
        let t = with_aliases(&[("caption", "xs"), ("body", "m")]);
        assert_eq!(t.resolve_space("caption").unwrap(), 2.0);
        assert_eq!(t.resolve_border_thickness("body").unwrap(), 1.5);
        assert_eq!(t.resolve_radius("body").unwrap(), 6.0);
        assert_eq!(t.resolve_width("caption").unwrap(), 120.0);
        assert_eq!(t.resolve_height("body").unwrap(), 32.0);
        assert_eq!(t.resolve_grid_col("body").unwrap(), 140.0);
        assert_eq!(t.resolve_text_size("caption").unwrap(), 7.0);
        assert_eq!(t.resolve_line_height("body").unwrap(), 1.2);
    }

    #[test]
    fn an_alias_is_shared_by_every_row() {
        let t = with_aliases(&[("caption", "xs")]);
        assert_eq!(t.resolve_text_size("caption").unwrap(), t.text_size[0]);
        assert_eq!(t.resolve_space("caption").unwrap(), t.space[0], "a gap takes the name of a text size");
    }

    #[test]
    fn an_alias_follows_a_redefined_row() {
        let t = Tokens { text_size: [8.0, 10.0, 12.0, 16.0, 22.0, 32.0], ..with_aliases(&[("body", "m")]) };
        assert_eq!(t.resolve_text_size("body").unwrap(), 12.0);
    }

    #[test]
    fn an_alias_is_found_in_a_spacing_and_a_border_shorthand() {
        let t = with_aliases(&[("tight", "xs"), ("roomy", "l")]);
        assert_eq!(t.resolve_spacing("tight roomy").unwrap(), [2.0, 14.0, 2.0, 14.0]);
        assert_eq!(t.resolve_border("roomy #ccc").unwrap(), (2.0, "#cccccc".to_string()));
    }

    #[test]
    fn a_name_that_is_neither_a_slot_nor_an_alias_says_what_is_expected() {
        let t = Tokens::default();
        let err = t.resolve_text_size("medum").unwrap_err();
        assert_eq!(err, "invalid text size: 'medum': expected a slot from xs to xxl or a pt value");
        let err = t.resolve_space("medum").unwrap_err();
        assert_eq!(err, "invalid space value: 'medum': expected a slot from xs to xxl or a pt value");

        let t = with_aliases(&[("caption", "xs"), ("body", "m")]);
        let err = t.resolve_space("medum").unwrap_err();
        assert_eq!(
            err,
            "invalid space value: 'medum': expected a slot from xs to xxl, an alias (body, caption) or a pt value"
        );
        let err = t.resolve_line_height("medum").unwrap_err();
        assert!(err.starts_with("invalid line-height 'medum': expected a slot from xs to xxl, an alias (body, caption)"), "{err}");
    }

    #[test]
    fn pt_values_and_factors_still_resolve_beside_aliases() {
        let t = with_aliases(&[("body", "m")]);
        assert_eq!(t.resolve_space("12pt").unwrap(), 12.0);
        assert_eq!(t.resolve_line_height("1.5").unwrap(), 1.5);
    }

    #[test]
    fn an_alias_takes_a_name_like_a_gradient_does() {
        let mut t = Tokens::default();
        for bad in ["", "Caption", "1st", "-caption", "cap tion", "cap_tion", "12pt"] {
            let err = t.add_alias(bad, "xs").unwrap_err();
            assert!(err.contains("lowercase letters, digits and hyphens"), "{bad}: {err}");
        }
        assert!(t.add_alias("sub-title-2", "l").is_ok());
    }

    #[test]
    fn an_alias_cannot_take_the_name_of_a_slot_or_a_keyword() {
        let mut t = Tokens::default();
        for slot in SCALE_NAMES {
            let err = t.add_alias(slot, "m").unwrap_err();
            assert!(err.contains("are the slots themselves"), "{slot}: {err}");
        }
        for keyword in ["fill", "full"] {
            let err = t.add_alias(keyword, "m").unwrap_err();
            assert!(err.contains("is a keyword"), "{keyword}: {err}");
        }
        assert!(t.aliases.is_empty());
    }

    #[test]
    fn an_alias_names_one_of_the_six_slots() {
        let mut t = Tokens::default();
        for bad in ["huge", "12pt", "caption", "", "M"] {
            let err = t.add_alias("body", bad).unwrap_err();
            assert_eq!(err, format!("alias 'body': invalid value '{bad}': expected a slot from xs to xxl"));
        }
        assert!(t.aliases.is_empty(), "an alias that failed is not kept");
    }

    #[test]
    fn an_alias_defined_twice_is_an_error() {
        let mut t = with_aliases(&[("body", "m")]);
        assert_eq!(t.add_alias("body", "l").unwrap_err(), "alias 'body': it is defined twice");
        assert_eq!(t.resolve_space("body").unwrap(), 8.0, "the first definition stands");
    }

    #[test]
    fn parse_opacity_takes_a_number_from_0_to_1() {
        for (val, want) in [("0", 0.0), ("1", 1.0), ("0.5", 0.5), (".25", 0.25), (" 0.5 ", 0.5)] {
            assert_eq!(parse_opacity(val).unwrap(), want, "{val}");
        }
        for bad in ["1.01", "-0.1", "half", "", "NaN", "inf", "50%"] {
            let err = parse_opacity(bad).unwrap_err();
            assert!(err.starts_with(&format!("invalid opacity '{bad}'")), "{bad}: {err}");
        }
    }

    #[test]
    fn the_default_line_height_row_has_the_default_factor_at_m() {
        assert_eq!(Tokens::default().line_height[scale_idx("m").unwrap()], crate::parse::DEFAULT_LINE_HEIGHT);
    }

    #[test]
    fn normalize_hex_short() {
        assert_eq!(normalize_hex("#fff").unwrap(), "#ffffff");
        assert_eq!(normalize_hex("#CCC").unwrap(), "#cccccc");
    }

    #[test]
    fn resolve_border_shorthand() {
        let t = Tokens::default();
        let (thickness, color) = t.resolve_border("s #ccc").unwrap();
        assert_eq!(thickness, 1.0);
        assert_eq!(color, "#cccccc");
    }

    #[test]
    fn resolve_color_named() {
        let t = Tokens::default();
        assert_eq!(t.resolve_color("primary").unwrap(), "#1763cf");
        assert_eq!(t.resolve_color("surface-alt").unwrap(), "#f5f5f5");
        assert_eq!(t.resolve_color("line").unwrap(), "#737373");
        assert_eq!(t.resolve_color("line-muted").unwrap(), "#d4d4d4");
    }

    // ── Gradients ─────────────────────────────────────────────────────────────

    fn spec<'a>(kind: &'a str, from: Option<&'a str>, to: Option<&'a str>) -> GradientSpec<'a> {
        GradientSpec { kind, angle: None, from, to, stops: Vec::new() }
    }

    fn stop_of(offset: f32, color: &str) -> GradientStop {
        GradientStop { offset, color: color.to_string() }
    }

    #[test]
    fn a_gradient_of_from_and_to_has_a_stop_at_each_end() {
        let gradient = Tokens::default().resolve_gradient("fade", &spec("linear", Some("#f00"), Some("#00f"))).unwrap();
        assert_eq!(gradient.kind, GradientKind::Linear);
        assert_eq!(gradient.angle, 0.0, "the default angle runs top to bottom");
        assert_eq!(gradient.stops, vec![stop_of(0.0, "#ff0000"), stop_of(1.0, "#0000ff")]);
    }

    #[test]
    fn a_gradient_takes_color_tokens_as_well_as_hex() {
        let gradient = Tokens::default().resolve_gradient("fade", &spec("radial", Some("primary"), Some("#ffffff"))).unwrap();
        assert_eq!(gradient.kind, GradientKind::Radial);
        assert_eq!(gradient.stops, vec![stop_of(0.0, "#1763cf"), stop_of(1.0, "#ffffff")]);
    }

    #[test]
    fn a_gradient_follows_a_color_token_the_document_redefines() {
        let mut tokens = Tokens::default();
        tokens.colors.insert("primary".into(), "#112233".into());
        let gradient = tokens.resolve_gradient("fade", &spec("linear", Some("primary"), Some("#fff"))).unwrap();
        assert_eq!(gradient.stops[0].color, "#112233");
    }

    #[test]
    fn stops_are_read_with_their_offsets_and_win_over_from_and_to() {
        let gradient = GradientSpec {
            kind: "linear",
            angle: Some("135"),
            from: Some("#000"),
            to: Some("#fff"),
            stops: vec![("0%", "#f97316"), ("60.5%", "#ec4899"), ("100%", "#8b5cf6")],
        };
        let gradient = Tokens::default().resolve_gradient("sunset", &gradient).unwrap();
        assert_eq!(gradient.angle, 135.0);
        assert_eq!(gradient.stops, vec![stop_of(0.0, "#f97316"), stop_of(0.605, "#ec4899"), stop_of(1.0, "#8b5cf6")]);
    }

    #[test]
    fn the_first_and_last_color_run_out_to_the_ends() {
        let gradient = GradientSpec { stops: vec![("20%", "#f00"), ("70%", "#00f")], ..spec("linear", None, None) };
        let gradient = Tokens::default().resolve_gradient("fade", &gradient).unwrap();
        assert_eq!(
            gradient.stops,
            vec![stop_of(0.0, "#ff0000"), stop_of(0.2, "#ff0000"), stop_of(0.7, "#0000ff"), stop_of(1.0, "#0000ff")]
        );
    }

    #[test]
    fn a_gradient_that_is_not_one_says_so() {
        let mut stops = spec("linear", None, None);
        stops.stops = vec![("0%", "#f00")];
        let mut backwards = spec("linear", None, None);
        backwards.stops = vec![("50%", "#f00"), ("50%", "#00f")];
        let mut outside = spec("linear", None, None);
        outside.stops = vec![("0%", "#f00"), ("120%", "#00f")];
        let mut no_percent = spec("linear", None, None);
        no_percent.stops = vec![("0%", "#f00"), ("0.5", "#00f")];
        let mut by_gradient = spec("linear", None, None);
        by_gradient.stops = vec![("0%", "#f00"), ("100%", "gradient:other")];
        let mut angled = spec("linear", Some("#f00"), Some("#00f"));
        angled.angle = Some("steep");

        let cases = [
            ("fade", spec("conic", Some("#f00"), Some("#00f")), "invalid type 'conic': expected linear or radial"),
            ("fade", spec("linear", Some("#f00"), None), "give from and to, or at least two stops"),
            ("fade", spec("linear", None, None), "give from and to, or at least two stops"),
            ("fade", stops, "a gradient needs at least two stops"),
            ("fade", backwards, "the stop offsets must increase, but 50% comes after 50%"),
            ("fade", outside, "invalid stop offset '120%': expected a percentage from 0% to 100%"),
            ("fade", no_percent, "invalid stop offset '0.5': expected a percentage from 0% to 100%"),
            ("fade", spec("linear", Some("#f00"), Some("nope")), "unknown color token: 'nope'"),
            ("fade", by_gradient, "is a gradient, which can fill a background or a shape"),
            ("fade", angled, "invalid angle 'steep': expected degrees, like 90"),
            ("Fade", spec("linear", Some("#f00"), Some("#00f")), "the name must be lowercase letters"),
            ("2fade", spec("linear", Some("#f00"), Some("#00f")), "the name must be lowercase letters"),
        ];
        for (name, gradient, expected) in cases {
            let err = Tokens::default().resolve_gradient(name, &gradient).unwrap_err();
            assert!(err.starts_with(&format!("gradient '{name}': ")) && err.contains(expected), "{expected}: {err}");
        }
    }

    #[test]
    fn a_fill_is_a_color_or_a_gradient_of_the_tokens() {
        let mut tokens = Tokens::default();
        tokens.gradients.insert(
            "fade".into(),
            Gradient { kind: GradientKind::Linear, angle: 0.0, stops: vec![stop_of(0.0, "#ff0000"), stop_of(1.0, "#0000ff")] },
        );
        assert_eq!(tokens.resolve_fill("#FFF").unwrap(), "#ffffff");
        assert_eq!(tokens.resolve_fill("primary").unwrap(), "#1763cf");
        assert_eq!(tokens.resolve_fill("gradient:fade").unwrap(), "gradient:fade");
        assert_eq!(tokens.resolve_fill("gradient:missing").unwrap_err(), "unknown gradient token: 'missing'");
    }

    #[test]
    fn a_gradient_is_not_a_color() {
        let mut tokens = Tokens::default();
        tokens.gradients.insert(
            "fade".into(),
            Gradient { kind: GradientKind::Linear, angle: 0.0, stops: vec![stop_of(0.0, "#ff0000"), stop_of(1.0, "#0000ff")] },
        );
        let err = tokens.resolve_color("gradient:fade").unwrap_err();
        assert!(err.contains("'gradient:fade' is a gradient") && err.contains("not color a text, a stroke or a border"), "{err}");
    }

    #[test]
    fn resolve_spacing_shorthand() {
        let t = Tokens::default();
        // single value: all four sides
        assert_eq!(t.resolve_spacing("m").unwrap(), [8.0, 8.0, 8.0, 8.0]);
        // two values: top/bottom, left/right
        assert_eq!(t.resolve_spacing("s m").unwrap(), [4.0, 8.0, 4.0, 8.0]);
        // a length and a slot in one shorthand, as the quote of the default Markdown styles has
        assert_eq!(t.resolve_spacing("0pt 0pt 0pt m").unwrap(), [0.0, 0.0, 0.0, 8.0]);
        assert_eq!(t.resolve_spacing("m 3pt xs").unwrap(), [8.0, 3.0, 2.0, 3.0]);
        // four explicit
        assert_eq!(
            t.resolve_spacing("2pt 4pt 6pt 8pt").unwrap(),
            [2.0, 4.0, 6.0, 8.0]
        );
    }
}
