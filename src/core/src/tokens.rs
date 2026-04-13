use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Tokens {
    pub space: [f32; 6],
    pub border: [f32; 6],
    pub radius: [f32; 6],
    pub width: [f32; 6],
    pub grid_col: [f32; 6],
    pub text: [f32; 6],
    pub colors: HashMap<String, String>,
    pub fonts: HashMap<String, FontDef>,
}

#[derive(Debug, Clone)]
pub enum FontDef {
    Builtin(String),
    Src(String),
}

const SCALE_NAMES: [&str; 6] = ["xs", "s", "m", "l", "xl", "xxl"];

fn scale_idx(name: &str) -> Option<usize> {
    SCALE_NAMES.iter().position(|&n| n == name)
}

/// Parse a pt value like "12pt" or "0.5pt"
pub fn parse_pt(val: &str) -> Option<f32> {
    val.trim().strip_suffix("pt")?.trim().parse().ok()
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

fn resolve_scale(val: &str, scale: &[f32; 6], name: &str) -> Result<f32, String> {
    if let Some(i) = scale_idx(val) {
        return Ok(scale[i]);
    }
    if let Some(v) = parse_pt(val) {
        return Ok(v);
    }
    Err(format!("invalid {name} value: '{val}'"))
}

impl Tokens {
    pub fn resolve_space(&self, val: &str) -> Result<f32, String> {
        resolve_scale(val, &self.space, "space")
    }

    pub fn resolve_border_thickness(&self, val: &str) -> Result<f32, String> {
        resolve_scale(val, &self.border, "border")
    }

    pub fn resolve_radius(&self, val: &str) -> Result<f32, String> {
        resolve_scale(val, &self.radius, "radius")
    }

    /// Width token OR explicit pt value
    pub fn resolve_width(&self, val: &str) -> Result<f32, String> {
        if let Some(i) = scale_idx(val) {
            return Ok(self.width[i]);
        }
        if let Some(v) = parse_pt(val) {
            return Ok(v);
        }
        Err(format!("invalid width value: '{val}'"))
    }

    /// Grid column min-width: scale token or pt value
    pub fn resolve_grid_col(&self, val: &str) -> Result<f32, String> {
        resolve_scale(val, &self.grid_col, "grid")
    }

    /// Text size: scale token, pt value, or plain number
    pub fn resolve_text_size(&self, val: &str) -> Result<f32, String> {
        if let Some(i) = scale_idx(val) {
            return Ok(self.text[i]);
        }
        if let Some(v) = parse_pt(val) {
            return Ok(v);
        }
        val.parse::<f32>()
            .map_err(|_| format!("invalid text size: '{val}'"))
    }

    pub fn resolve_color(&self, val: &str) -> Result<String, String> {
        if val.starts_with('#') {
            return normalize_hex(val);
        }
        self.colors
            .get(val)
            .cloned()
            .ok_or_else(|| format!("unknown color token: '{val}'"))
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
        ] {
            colors.insert(name.to_string(), val.to_string());
        }
        Tokens {
            space: [2.0, 4.0, 8.0, 14.0, 20.0, 32.0],
            border: [0.5, 1.0, 1.5, 2.0, 3.0, 4.0],
            radius: [2.0, 4.0, 6.0, 10.0, 16.0, 24.0],
            width: [120.0, 200.0, 280.0, 360.0, 440.0, 515.0],
            grid_col: [60.0, 100.0, 140.0, 180.0, 220.0, 280.0],
            text: [7.0, 9.0, 11.0, 14.0, 20.0, 28.0],
            colors,
            fonts: HashMap::new(),
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
    }

    #[test]
    fn resolve_spacing_shorthand() {
        let t = Tokens::default();
        // single value: all four sides
        assert_eq!(t.resolve_spacing("m").unwrap(), [8.0, 8.0, 8.0, 8.0]);
        // two values: top/bottom, left/right
        assert_eq!(t.resolve_spacing("s m").unwrap(), [4.0, 8.0, 4.0, 8.0]);
        // four explicit
        assert_eq!(
            t.resolve_spacing("2pt 4pt 6pt 8pt").unwrap(),
            [2.0, 4.0, 6.0, 8.0]
        );
    }
}
