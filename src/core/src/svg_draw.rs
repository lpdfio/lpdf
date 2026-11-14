//! The tree `usvg` reads from an SVG, written as PDF content (`svg.rs`).
//!
//! The picture is one form XObject whose box is the unit square, so the layout draws it where it draws a JPEG or a
//! PNG: it scales the unit square with the CTM. A group faded as a whole, which is the opacity of its shapes
//! together and no shape's own, is a transparency group of its own, one more form the first invokes.
//!
//! What the writer can't draw it refuses, with an error that names the tag and, where the file has it, the line:
//! a mask, a filter, a pattern, a gradient (until gradients are drawn), a blend mode, a clip path that clips a clip
//! path's child, and a clip path whose shapes mix fill rules or overlap.

use pdf_writer::types::{LineCapStyle, LineJoinStyle};
use pdf_writer::{Content, Name};
use roxmltree21::Document;
use usvg::tiny_skia_path::{PathSegment, Point, Rect, Transform};
use usvg::{BlendMode, ClipPath, FillRule, Group, LineCap, LineJoin, Node, Paint, PaintOrder, Path, Tree};

use crate::svg_scan::{line_of, refusal};

/// A form of the picture: its content and its box, in the coordinates its content is written in.
pub(crate) struct DrawnForm {
    pub(crate) content: Vec<u8>,
    pub(crate) bbox: [f32; 4],
    /// The transparency groups the content invokes, by the index of their form: the resources name no other.
    pub(crate) groups: Vec<usize>,
}

/// What the writer made of a tree.
pub(crate) struct Drawn {
    /// The picture first, then the transparency groups it invokes, each named `G<index>`.
    pub(crate) forms: Vec<DrawnForm>,
    /// The fill and stroke alpha of each graphics state the forms ask for, each named `S<index>`.
    pub(crate) states: Vec<(f32, f32)>,
    /// Some shape is painted in the color the page has set when the picture is drawn.
    pub(crate) current_color: bool,
}

pub(crate) fn state_name(index: usize) -> String {
    format!("S{index}")
}

pub(crate) fn form_name(index: usize) -> String {
    format!("G{index}")
}

/// Write `tree`. `stand_in` is the color `usvg` was given for `currentColor`: a paint of exactly that color is left
/// to the page's.
pub(crate) fn draw(tree: &Tree, doc: &Document, stand_in: Option<(u8, u8, u8)>) -> Result<Drawn, String> {
    let size = tree.size();
    let mut pen = Pen {
        doc,
        stand_in,
        forms: vec![DrawnForm { content: Vec::new(), bbox: [0.0, 0.0, 1.0, 1.0], groups: Vec::new() }],
        writing: 0,
        states: Vec::new(),
        current_color: false,
    };
    let mut content = Content::new();
    // The SVG's space has y down and spans the size of the image; the form's is the unit square with y up.
    content.transform([1.0 / size.width(), 0.0, 0.0, -1.0 / size.height(), 0.0, 1.0]);
    pen.group(tree.root(), &mut content)?;
    pen.forms[0].content = content.finish().to_vec();
    Ok(Drawn { forms: pen.forms, states: pen.states, current_color: pen.current_color })
}

/// How a shape is painted: with a color of its own, or with the page's.
enum Ink {
    Rgb(f32, f32, f32),
    Page,
}

struct Pen<'a, 'input> {
    doc: &'a Document<'input>,
    stand_in: Option<(u8, u8, u8)>,
    forms: Vec<DrawnForm>,
    /// The form whose content is being written.
    writing: usize,
    states: Vec<(f32, f32)>,
    current_color: bool,
}

impl Pen<'_, '_> {
    /// Draw a group: its transform, its clip, then its children, faded as a whole when it has an opacity.
    fn group(&mut self, group: &Group, content: &mut Content) -> Result<(), String> {
        if let Some(mask) = group.mask() {
            return Err(self.cannot("a <mask>", mask.id(), "can't be drawn"));
        }
        if let Some(filter) = group.filters().first() {
            return Err(self.cannot("a <filter>", filter.id(), "can't be drawn"));
        }
        if group.blend_mode() != BlendMode::Normal {
            return Err(refusal("a mix-blend-mode", &line_of(self.doc, group.id()), "can't be drawn"));
        }
        let opacity = group.opacity().get();
        if opacity <= 0.0 {
            return Ok(());
        }
        content.save_state();
        let transform = group.transform();
        if !transform.is_identity() {
            content.transform(matrix(&transform));
        }
        if let Some(clip) = group.clip_path() {
            self.clip(clip, content)?;
        }
        if opacity >= 1.0 {
            self.children(group, 1.0, content)?;
        } else if let [Node::Path(only)] = group.children()
            && (only.fill().is_none() || only.stroke().is_none())
        {
            // A shape with a fill or a stroke alone is faded the same by its own alpha.
            self.children(group, opacity, content)?;
        } else {
            self.faded(group, opacity, content)?;
        }
        content.restore_state();
        Ok(())
    }

    /// Draw the children of `group`, each shape's alpha scaled by `scale`.
    fn children(&mut self, group: &Group, scale: f32, content: &mut Content) -> Result<(), String> {
        for child in group.children() {
            crate::deadline::check();
            crate::memory::check()?;
            match child {
                Node::Group(inner) => self.group(inner, content)?,
                Node::Path(path) => self.path(path, scale, content)?,
                Node::Image(_) => return Err(refusal("an <image>", "", "can't be drawn: an SVG can't hold a picture")),
                Node::Text(_) => {
                    return Err(refusal("a <text>", "", "can't be drawn yet: convert the text to outlines"));
                }
            }
        }
        Ok(())
    }

    /// Draw `group` as a transparency group of its own, which the page composites first and fades as one.
    fn faded(&mut self, group: &Group, opacity: f32, content: &mut Content) -> Result<(), String> {
        let index = self.forms.len();
        self.forms.push(DrawnForm { content: Vec::new(), bbox: [0.0; 4], groups: Vec::new() });
        self.forms[self.writing].groups.push(index);
        let outer = std::mem::replace(&mut self.writing, index);
        let mut inner = Content::new();
        self.children(group, 1.0, &mut inner)?;
        self.writing = outer;
        let area = group.layer_bounding_box();
        // A pixel more on each side, so that the edge's antialiasing is not cut off.
        self.forms[index].content = inner.finish().to_vec();
        self.forms[index].bbox = [area.left() - 1.0, area.top() - 1.0, area.right() + 1.0, area.bottom() + 1.0];
        let state = self.state(opacity, opacity);
        content.set_parameters(Name(state_name(state).as_bytes()));
        content.x_object(Name(form_name(index).as_bytes()));
        Ok(())
    }

    /// Intersect the clip with the shapes of `clip`, which are in the space the group is drawn in moved by the
    /// clip's own transform. The shapes are united by writing them as the subpaths of one path, which holds only where
    /// no two overlap: one that winds the other way, or any under the even-odd rule, would leave a hole where they
    /// meet. A clip whose shapes' boxes overlap is refused.
    fn clip(&mut self, clip: &ClipPath, content: &mut Content) -> Result<(), String> {
        if let Some(outer) = clip.clip_path() {
            self.clip(outer, content)?;
        }
        let mut rule = None;
        let mut areas = Vec::new();
        self.clip_shapes(clip, clip.root(), &clip.transform(), (&mut rule, &mut areas), content)?;
        match rule {
            Some(FillRule::EvenOdd) => content.clip_even_odd(),
            Some(FillRule::NonZero) => content.clip_nonzero(),
            None => {
                // A clip with no shape clips everything away.
                content.rect(0.0, 0.0, 0.0, 0.0);
                content.clip_nonzero()
            }
        };
        content.end_path();
        Ok(())
    }

    fn clip_shapes(
        &mut self,
        clip: &ClipPath,
        group: &Group,
        transform: &Transform,
        (rule, areas): (&mut Option<FillRule>, &mut Vec<Rect>),
        content: &mut Content,
    ) -> Result<(), String> {
        let transform = transform.pre_concat(group.transform());
        if group.clip_path().is_some() {
            return Err(self.cannot("a clip-path on a child of a <clipPath>", clip.id(), "can't be drawn"));
        }
        for child in group.children() {
            crate::deadline::check();
            match child {
                Node::Group(inner) => self.clip_shapes(clip, inner, &transform, (&mut *rule, &mut *areas), content)?,
                Node::Path(path) => {
                    let this = path.fill().map_or(FillRule::NonZero, |fill| fill.rule());
                    if *rule.get_or_insert(this) != this {
                        return Err(self.cannot("a <clipPath> whose shapes mix clip-rule values", clip.id(), "can't be drawn"));
                    }
                    if let Some(area) = path.data().bounds().transform(transform) {
                        if areas.iter().any(|other| overlap(other, &area)) {
                            return Err(self.cannot("a <clipPath> whose shapes overlap", clip.id(), "can't be drawn"));
                        }
                        areas.push(area);
                    }
                    outline(content, path, &transform);
                }
                Node::Image(_) | Node::Text(_) => {}
            }
        }
        Ok(())
    }

    fn path(&mut self, path: &Path, scale: f32, content: &mut Content) -> Result<(), String> {
        if !path.is_visible() {
            return Ok(());
        }
        let fill = match path.fill() {
            Some(fill) => Some((self.ink(fill.paint())?, fill.opacity().get() * scale, fill.rule())),
            None => None,
        };
        let stroke = match path.stroke() {
            Some(stroke) => Some((self.ink(stroke.paint())?, stroke.opacity().get() * scale, stroke)),
            None => None,
        };
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        content.save_state();
        let fill_alpha = fill.as_ref().map_or(1.0, |(_, alpha, _)| *alpha);
        let stroke_alpha = stroke.as_ref().map_or(1.0, |(_, alpha, _)| *alpha);
        if fill_alpha < 1.0 || stroke_alpha < 1.0 {
            let state = self.state(fill_alpha, stroke_alpha);
            content.set_parameters(Name(state_name(state).as_bytes()));
        }
        if let Some((Ink::Rgb(r, g, b), _, _)) = &fill {
            content.set_fill_rgb(*r, *g, *b);
        }
        if let Some((ink, _, stroke)) = &stroke {
            if let Ink::Rgb(r, g, b) = ink {
                content.set_stroke_rgb(*r, *g, *b);
            }
            set_stroke_style(content, stroke);
        }
        paint(content, path, fill.as_ref().map(|(_, _, rule)| *rule), stroke.is_some(), (fill_alpha, stroke_alpha));
        content.restore_state();
        Ok(())
    }

    /// How a paint is set: its color, or the page's when it is the color `usvg` was given for `currentColor`.
    fn ink(&mut self, paint: &Paint) -> Result<Ink, String> {
        match paint {
            Paint::Color(color) => {
                if self.stand_in == Some((color.red, color.green, color.blue)) {
                    self.current_color = true;
                    return Ok(Ink::Page);
                }
                Ok(Ink::Rgb(f32::from(color.red) / 255.0, f32::from(color.green) / 255.0, f32::from(color.blue) / 255.0))
            }
            Paint::LinearGradient(gradient) => {
                Err(self.cannot("a <linearGradient>", gradient.id(), "can't be drawn yet"))
            }
            Paint::RadialGradient(gradient) => {
                Err(self.cannot("a <radialGradient>", gradient.id(), "can't be drawn yet"))
            }
            Paint::Pattern(pattern) => Err(self.cannot("a <pattern>", pattern.id(), "can't be drawn")),
        }
    }

    /// The refusal of `what`, found in the file by the `id` of the element `usvg` read it from.
    fn cannot(&self, what: &str, id: &str, why: &str) -> String {
        refusal(what, &line_of(self.doc, id), why)
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
}

/// Whether two boxes share more than an edge.
fn overlap(a: &Rect, b: &Rect) -> bool {
    a.left() < b.right() && b.left() < a.right() && a.top() < b.bottom() && b.top() < a.bottom()
}

/// Set the width, caps, joins and dashes a stroke draws with.
fn set_stroke_style(content: &mut Content, stroke: &usvg::Stroke) {
    content.set_line_width(stroke.width().get());
    content.set_line_cap(match stroke.linecap() {
        LineCap::Butt => LineCapStyle::ButtCap,
        LineCap::Round => LineCapStyle::RoundCap,
        LineCap::Square => LineCapStyle::ProjectingSquareCap,
    });
    content.set_line_join(match stroke.linejoin() {
        LineJoin::Miter | LineJoin::MiterClip => LineJoinStyle::MiterJoin,
        LineJoin::Round => LineJoinStyle::RoundJoin,
        LineJoin::Bevel => LineJoinStyle::BevelJoin,
    });
    content.set_miter_limit(stroke.miterlimit().get());
    if let Some(dashes) = stroke.dasharray() {
        content.set_dash_pattern(dashes.iter().copied(), stroke.dashoffset());
    }
}

/// Paint the outline of `path`: its fill (with `rule`) and its stroke, in the order the SVG asks for. A shape both
/// filled and stroked in full is painted in one operator; any alpha makes two, so that each is faded alone.
fn paint(content: &mut Content, path: &Path, rule: Option<FillRule>, stroked: bool, alphas: (f32, f32)) {
    let identity = Transform::identity();
    let both_opaque = alphas.0 >= 1.0 && alphas.1 >= 1.0;
    let stroke_first = path.paint_order() == PaintOrder::StrokeAndFill;
    if let (Some(rule), true, true, false) = (rule, stroked, both_opaque, stroke_first) {
        outline(content, path, &identity);
        match rule {
            FillRule::NonZero => content.fill_nonzero_and_stroke(),
            FillRule::EvenOdd => content.fill_even_odd_and_stroke(),
        };
        return;
    }
    for fill_turn in if stroke_first { [false, true] } else { [true, false] } {
        if fill_turn {
            if let Some(rule) = rule {
                outline(content, path, &identity);
                match rule {
                    FillRule::NonZero => content.fill_nonzero(),
                    FillRule::EvenOdd => content.fill_even_odd(),
                };
            }
        } else if stroked {
            outline(content, path, &identity);
            content.stroke();
        }
    }
}

/// The outline of `path` moved by `transform`, without painting it.
fn outline(content: &mut Content, path: &Path, transform: &Transform) {
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
                content.move_to(x, y);
                current = to;
            }
            PathSegment::LineTo(to) => {
                let (x, y) = at(to);
                content.line_to(x, y);
                current = to;
            }
            PathSegment::QuadTo(control, to) => {
                // A quadratic curve as the cubic with the same shape.
                let first = Point::from_xy(
                    current.x + 2.0 / 3.0 * (control.x - current.x),
                    current.y + 2.0 / 3.0 * (control.y - current.y),
                );
                let second =
                    Point::from_xy(to.x + 2.0 / 3.0 * (control.x - to.x), to.y + 2.0 / 3.0 * (control.y - to.y));
                let ((x1, y1), (x2, y2), (x3, y3)) = (at(first), at(second), at(to));
                content.cubic_to(x1, y1, x2, y2, x3, y3);
                current = to;
            }
            PathSegment::CubicTo(first, second, to) => {
                let ((x1, y1), (x2, y2), (x3, y3)) = (at(first), at(second), at(to));
                content.cubic_to(x1, y1, x2, y2, x3, y3);
                current = to;
            }
            PathSegment::Close => {
                content.close_path();
            }
        }
    }
}

/// `transform` as the six numbers of a PDF matrix.
fn matrix(transform: &Transform) -> [f32; 6] {
    [transform.sx, transform.ky, transform.kx, transform.sy, transform.tx, transform.ty]
}
