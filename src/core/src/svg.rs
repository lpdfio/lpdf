//! SVG images, drawn as vector. PROTOTYPE of the route "`usvg` + our own writer": it settles the size and the cost
//! of the route, and is not the finished feature (no pre-scan, no limits, no gradients, no text).
//!
//! An SVG handed to `load_image` is read by `usvg`, which resolves `use`, `<style>`, units and markers into a tree of
//! groups and paths. `embed` writes that tree as a form XObject whose box is the unit square, so the image draws where
//! a JPEG or a PNG would: the layout scales the unit square with the CTM, as it does for every image.

use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref};
use usvg::tiny_skia_path::{PathSegment, Point, Transform};
use usvg::{ClipPath, FillRule, Group, LineCap, LineJoin, Node, Paint, PaintOrder, Path, Tree};

/// Whether `bytes` are an SVG document: after an optional byte order mark, whitespace, an XML declaration, comments
/// and a doctype, the root element is `svg`.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let mut rest = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    loop {
        rest = rest.trim_ascii_start();
        let skipped = if rest.starts_with(b"<?") {
            skip_past(rest, b"?>")
        } else if rest.starts_with(b"<!--") {
            skip_past(rest, b"-->")
        } else if rest.starts_with(b"<!DOCTYPE") {
            skip_doctype(rest)
        } else {
            return rest.starts_with(b"<svg") && rest.get(4).is_some_and(|c| c.is_ascii_whitespace() || *c == b'>');
        };
        match skipped {
            Some(after) => rest = after,
            None => return false,
        }
    }
}

/// What follows the first `end` in `bytes`.
fn skip_past<'a>(bytes: &'a [u8], end: &[u8]) -> Option<&'a [u8]> {
    bytes.windows(end.len()).position(|window| window == end).map(|at| &bytes[at + end.len()..])
}

/// What follows a doctype, whose internal subset in `[` `]` may hold a `>`.
fn skip_doctype(bytes: &[u8]) -> Option<&[u8]> {
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b'>' if depth == 0 => return Some(&bytes[at + 1..]),
            _ => {}
        }
    }
    None
}

fn parse(bytes: &[u8]) -> Result<Tree, String> {
    Tree::from_data(bytes, &usvg::Options::default()).map_err(|e| format!("the SVG could not be read: {e}"))
}

/// The size of the SVG in pixels, as its root `width` and `height` say, else its `viewBox`.
pub(crate) fn natural_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let size = parse(bytes).ok()?.size();
    Some(((size.width().ceil() as u32).max(1), (size.height().ceil() as u32).max(1)))
}

/// Why an SVG can't be drawn, or `None` when it can.
pub(crate) fn error(bytes: &[u8]) -> Option<String> {
    parse(bytes).err()
}

/// The content of an SVG's form XObject, and the graphics states its paths ask for.
struct Form {
    content: Vec<u8>,
    /// The fill and stroke alpha of each `gs` the content invokes, as `S<index>`.
    states: Vec<(f32, f32)>,
}

/// Write the SVG in `bytes` as the form XObject `id`.
pub(crate) fn embed(pdf: &mut Pdf, id: Ref, bytes: &[u8]) -> Result<(), String> {
    let form = draw(bytes)?;
    let stream = compress_to_vec_zlib(&form.content, 6);
    let mut xobject = pdf.form_xobject(id, &stream);
    xobject.filter(Filter::FlateDecode);
    xobject.bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
    if !form.states.is_empty() {
        let mut resources = xobject.resources();
        let mut states = resources.ext_g_states();
        for (index, (fill, stroke)) in form.states.iter().enumerate() {
            let name = state_name(index);
            let mut state = states.insert(Name(name.as_bytes())).start::<pdf_writer::writers::ExtGraphicsState>();
            state.non_stroking_alpha(*fill);
            state.stroking_alpha(*stroke);
        }
    }
    Ok(())
}

fn state_name(index: usize) -> String {
    format!("S{index}")
}

fn draw(bytes: &[u8]) -> Result<Form, String> {
    let tree = parse(bytes)?;
    let size = tree.size();
    let mut pen = Pen { content: Content::new(), states: Vec::new() };
    // The SVG's space has y down and spans the size of the image; the form's is the unit square with y up.
    pen.content.transform([1.0 / size.width(), 0.0, 0.0, -1.0 / size.height(), 0.0, 1.0]);
    pen.group(tree.root(), 1.0)?;
    Ok(Form { content: pen.content.finish().to_vec(), states: pen.states })
}

struct Pen {
    content: Content,
    states: Vec<(f32, f32)>,
}

impl Pen {
    fn group(&mut self, group: &Group, alpha: f32) -> Result<(), String> {
        if group.mask().is_some() {
            return Err("the SVG has a <mask>, which can't be drawn".to_string());
        }
        if !group.filters().is_empty() {
            return Err("the SVG has a filter, which can't be drawn".to_string());
        }
        let opacity = group.opacity().get();
        let alpha = if opacity < 1.0 {
            // A group's opacity is that of its shapes together; only one shape is the same as its own.
            if !matches!(group.children(), [Node::Path(_)]) {
                return Err("opacity over several shapes can't be drawn yet".to_string());
            }
            alpha * opacity
        } else {
            alpha
        };
        self.content.save_state();
        let transform = group.transform();
        if !transform.is_identity() {
            self.content.transform(matrix(&transform));
        }
        if let Some(clip) = group.clip_path() {
            self.clip(clip)?;
        }
        for child in group.children() {
            match child {
                Node::Group(inner) => self.group(inner, alpha)?,
                Node::Path(path) => self.path(path, alpha)?,
                _ => {}
            }
        }
        self.content.restore_state();
        Ok(())
    }

    /// Intersect the clip with the shapes of `clip`, which are in the space the group is drawn in moved by the
    /// clip's own transform. The shapes are united: one fill rule, the first one's.
    fn clip(&mut self, clip: &ClipPath) -> Result<(), String> {
        if let Some(outer) = clip.clip_path() {
            self.clip(outer)?;
        }
        let mut rule = None;
        self.clip_shapes(clip.root(), &clip.transform(), &mut rule)?;
        match rule {
            Some(FillRule::EvenOdd) => {
                self.content.clip_even_odd();
            }
            Some(FillRule::NonZero) => {
                self.content.clip_nonzero();
            }
            None => {
                // A clip with no shape clips everything away.
                self.content.rect(0.0, 0.0, 0.0, 0.0);
                self.content.clip_nonzero();
            }
        }
        self.content.end_path();
        Ok(())
    }

    fn clip_shapes(&mut self, group: &Group, transform: &Transform, rule: &mut Option<FillRule>) -> Result<(), String> {
        let transform = transform.pre_concat(group.transform());
        if group.clip_path().is_some() {
            return Err("a clip path inside a clip path can't be drawn yet".to_string());
        }
        for child in group.children() {
            match child {
                Node::Group(inner) => self.clip_shapes(inner, &transform, rule)?,
                Node::Path(path) => {
                    rule.get_or_insert_with(|| path.fill().map_or(FillRule::NonZero, |fill| fill.rule()));
                    self.outline(path, &transform);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn path(&mut self, path: &Path, alpha: f32) -> Result<(), String> {
        if !path.is_visible() {
            return Ok(());
        }
        let fill = match path.fill() {
            Some(fill) => Some((color(fill.paint())?, fill.opacity().get() * alpha, fill.rule())),
            None => None,
        };
        let stroke = match path.stroke() {
            Some(stroke) => Some((color(stroke.paint())?, stroke.opacity().get() * alpha, stroke)),
            None => None,
        };
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        self.content.save_state();
        let fill_alpha = fill.map_or(1.0, |(_, alpha, _)| alpha);
        let stroke_alpha = stroke.map_or(1.0, |(_, alpha, _)| alpha);
        if fill_alpha < 1.0 || stroke_alpha < 1.0 {
            let index = self.state(fill_alpha, stroke_alpha);
            self.content.set_parameters(Name(state_name(index).as_bytes()));
        }
        if let Some(((r, g, b), _, _)) = fill {
            self.content.set_fill_rgb(r, g, b);
        }
        if let Some(((r, g, b), _, stroke)) = stroke {
            self.content.set_stroke_rgb(r, g, b);
            self.content.set_line_width(stroke.width().get());
            self.content.set_line_cap(match stroke.linecap() {
                LineCap::Butt => LineCapStyle::ButtCap,
                LineCap::Round => LineCapStyle::RoundCap,
                LineCap::Square => LineCapStyle::ProjectingSquareCap,
            });
            self.content.set_line_join(match stroke.linejoin() {
                LineJoin::Miter | LineJoin::MiterClip => LineJoinStyle::MiterJoin,
                LineJoin::Round => LineJoinStyle::RoundJoin,
                LineJoin::Bevel => LineJoinStyle::BevelJoin,
            });
            self.content.set_miter_limit(stroke.miterlimit().get());
            if let Some(dashes) = stroke.dasharray() {
                self.content.set_dash_pattern(dashes.iter().copied(), stroke.dashoffset());
            }
        }
        let identity = Transform::identity();
        let stroke_first = path.paint_order() == PaintOrder::StrokeAndFill;
        match (fill, stroke) {
            (Some((_, _, rule)), Some(_)) if !stroke_first && fill_alpha >= 1.0 && stroke_alpha >= 1.0 => {
                self.outline(path, &identity);
                match rule {
                    FillRule::NonZero => self.content.fill_nonzero_and_stroke(),
                    FillRule::EvenOdd => self.content.fill_even_odd_and_stroke(),
                };
            }
            (fill, stroke) => {
                // Two paintings of one outline, in the order the SVG asks for.
                for painting in if stroke_first { [false, true] } else { [true, false] } {
                    match (painting, fill, stroke) {
                        (true, Some((_, _, rule)), _) => {
                            self.outline(path, &identity);
                            match rule {
                                FillRule::NonZero => self.content.fill_nonzero(),
                                FillRule::EvenOdd => self.content.fill_even_odd(),
                            };
                        }
                        (false, _, Some(_)) => {
                            self.outline(path, &identity);
                            self.content.stroke();
                        }
                        _ => {}
                    }
                }
            }
        }
        self.content.restore_state();
        Ok(())
    }

    /// The index of the graphics state of these alphas, added when it is new.
    fn state(&mut self, fill: f32, stroke: f32) -> usize {
        match self.states.iter().position(|known| *known == (fill, stroke)) {
            Some(index) => index,
            None => {
                self.states.push((fill, stroke));
                self.states.len() - 1
            }
        }
    }

    /// The outline of `path` moved by `transform`, without painting it.
    fn outline(&mut self, path: &Path, transform: &Transform) {
        let at = |point: Point| {
            (
                transform.sx * point.x + transform.kx * point.y + transform.tx,
                transform.ky * point.x + transform.sy * point.y + transform.ty,
            )
        };
        let mut current = Point::zero();
        for segment in path.data().segments() {
            match segment {
                PathSegment::MoveTo(to) => {
                    let (x, y) = at(to);
                    self.content.move_to(x, y);
                    current = to;
                }
                PathSegment::LineTo(to) => {
                    let (x, y) = at(to);
                    self.content.line_to(x, y);
                    current = to;
                }
                PathSegment::QuadTo(control, to) => {
                    // A quadratic curve as the cubic with the same shape.
                    let first = Point::from_xy(current.x + 2.0 / 3.0 * (control.x - current.x), current.y + 2.0 / 3.0 * (control.y - current.y));
                    let second = Point::from_xy(to.x + 2.0 / 3.0 * (control.x - to.x), to.y + 2.0 / 3.0 * (control.y - to.y));
                    let ((x1, y1), (x2, y2), (x3, y3)) = (at(first), at(second), at(to));
                    self.content.cubic_to(x1, y1, x2, y2, x3, y3);
                    current = to;
                }
                PathSegment::CubicTo(first, second, to) => {
                    let ((x1, y1), (x2, y2), (x3, y3)) = (at(first), at(second), at(to));
                    self.content.cubic_to(x1, y1, x2, y2, x3, y3);
                    current = to;
                }
                PathSegment::Close => {
                    self.content.close_path();
                }
            }
        }
    }
}

/// The color of a paint, as 0 to 1 RGB. A gradient or a pattern is not drawn yet.
fn color(paint: &Paint) -> Result<(f32, f32, f32), String> {
    match paint {
        Paint::Color(color) => Ok((f32::from(color.red) / 255.0, f32::from(color.green) / 255.0, f32::from(color.blue) / 255.0)),
        Paint::LinearGradient(_) | Paint::RadialGradient(_) => Err("an SVG gradient can't be drawn yet".to_string()),
        Paint::Pattern(_) => Err("an SVG <pattern> can't be drawn".to_string()),
    }
}

/// `transform` as the six numbers of a PDF matrix.
fn matrix(transform: &Transform) -> [f32; 6] {
    [transform.sx, transform.ky, transform.kx, transform.sy, transform.tx, transform.ty]
}
