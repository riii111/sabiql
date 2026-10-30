use rataflow::{
    EdgeContent, EdgePathContext, EdgeRenderContext, EdgeStyle, HandlePosition, NodeContent,
    NodeRenderContext, Path, Position,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::primitives::utils::text_utils::truncate_to_width_with;

pub(super) const SELF_REFERENCE_MARK: &str = "↻";
// Straight run out of a table before an edge may turn.
pub(super) const EDGE_STEM: f64 = 1.0;
// How far before its target an edge turns in; FKs into one table merge only here.
pub(super) const EDGE_APPROACH: f64 = 2.0;
// Rounds down to one cell inside each corner at full size.
const CORNER_INSET: f64 = 1.5;

/// A table box. When zoomed out too far for a box with text inside, the name
/// moves onto the border so tables stay identifiable.
#[derive(Debug)]
pub(super) struct TableNode {
    pub(super) name: String,
    /// Shown only when the diagram spans several schemas.
    pub(super) schema: Option<String>,
    pub(super) self_reference_fks: Vec<String>,
}

impl NodeContent for TableNode {
    fn render(&self, ctx: &NodeRenderContext, buf: &mut Buffer) {
        let palette = ctx.theme.palette();
        let border = if ctx.selected {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.muted)
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border)
            .bg(palette.surface);
        let text = Style::default().fg(palette.text);

        if ctx.area.height < 3 {
            // Too small for text inside: the name takes the whole top row,
            // corners included, and shows an ellipsis when it is cut.
            let area = ctx.area;
            block.render(area, buf);
            let name = truncate_to_width_with(&self.name, area.width as usize, "…");
            buf.set_string(area.x, area.y, " ".repeat(area.width as usize), border);
            buf.set_string(area.x, area.y, name, text);
            return;
        }

        let block = match &self.schema {
            Some(schema) => block.title(format!(" {schema} ")),
            None => block,
        };
        let inner = block.inner(ctx.area);
        block.render(ctx.area, buf);

        let mut lines = vec![Line::styled(self.name.as_str(), text)];
        lines.extend(self.self_reference_fks.iter().map(|name| {
            Line::styled(
                format!("{SELF_REFERENCE_MARK} {name}"),
                Style::default().fg(palette.muted),
            )
        }));
        let top_padding = inner.height.saturating_sub(lines.len() as u16) / 2;
        let text_area = Rect {
            y: inner.y + top_padding,
            height: inner.height - top_padding,
            ..inner
        };
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .render(text_area, buf);
    }
}

/// The edge for every FK between two tables, in either direction.
///
/// It leaves the source's facing side toward the target and keeps to that lane
/// until just before the target, then turns into the middle of the target's
/// facing side. FKs into one table therefore merge only next to their shared
/// arrowhead, and where each one comes from stays visible.
#[derive(Debug, Default)]
pub(super) struct FkEdge {
    pub(super) label: String,
    /// FKs run both ways between the two tables.
    pub(super) bidirectional: bool,
    pub(super) style: EdgeStyle,
}

impl EdgeContent for FkEdge {
    fn compute_path(&self, ctx: &EdgePathContext) -> Path {
        // Only a connection preview has no target, and the viewer never shows one.
        let Some(target) = ctx.target_bounds else {
            return Path::straight(ctx.from, ctx.to);
        };
        let source = ctx.source_bounds;
        let source_side = HandlePosition::facing(&source, target.center());
        let target_side = HandlePosition::facing(&target, source.center());
        let from = departure_point(&source, source_side, target.center());
        let to = side_midpoint(&target, target_side);

        // Facing sides closer than a stem plus an approach would fold back on
        // themselves, so they split the gap instead.
        let (stem, approach) = match gap_between(from, to, source_side, target_side) {
            Some(gap) if gap < EDGE_STEM + EDGE_APPROACH => {
                let half = (gap / 2.0).max(0.0);
                (half, half)
            }
            _ => (EDGE_STEM, EDGE_APPROACH),
        };
        let lane_start = step(from, source_side, stem);
        let turn_in = step(to, target_side, approach);
        let bend = if target_side.is_horizontal() {
            Position::new(turn_in.x, lane_start.y)
        } else {
            Position::new(lane_start.x, turn_in.y)
        };
        let mut points = vec![from, lane_start, bend, turn_in, to];
        points.dedup();
        let label_position = Position::new(
            f64::midpoint(lane_start.x, bend.x),
            f64::midpoint(lane_start.y, bend.y),
        );
        Path::new(points, source_side, target_side).with_label_position(label_position)
    }

    fn render(&self, ctx: &EdgeRenderContext, buf: &mut Buffer) {
        let label = ctx.label.map(Text::raw);
        ctx.render_path(&self.style, label.as_ref(), buf);
    }
}

/// Distance between `from` and `to` along the axis of two facing sides.
fn gap_between(
    from: Position,
    to: Position,
    source_side: HandlePosition,
    target_side: HandlePosition,
) -> Option<f64> {
    if source_side != target_side.opposite() {
        return None;
    }
    Some(match source_side {
        HandlePosition::Right => to.x - from.x,
        HandlePosition::Left => from.x - to.x,
        HandlePosition::Bottom => to.y - from.y,
        HandlePosition::Top => from.y - to.y,
    })
}

/// `point` moved `distance` outward from the `side` it sits on.
fn step(point: Position, side: HandlePosition, distance: f64) -> Position {
    match side {
        HandlePosition::Top => Position::new(point.x, point.y - distance),
        HandlePosition::Bottom => Position::new(point.x, point.y + distance),
        HandlePosition::Left => Position::new(point.x - distance, point.y),
        HandlePosition::Right => Position::new(point.x + distance, point.y),
    }
}

fn side_midpoint(bounds: &rataflow::Rect, side: HandlePosition) -> Position {
    let center = bounds.center();
    match side {
        HandlePosition::Top => Position::new(center.x, bounds.y()),
        HandlePosition::Bottom => Position::new(center.x, bounds.bottom()),
        HandlePosition::Left => Position::new(bounds.x(), center.y),
        HandlePosition::Right => Position::new(bounds.right(), center.y),
    }
}

/// Where an edge toward `toward` leaves `bounds` on `side`: where a ray from the
/// center would cross that side, kept off the corners so the edge never runs
/// along the border itself.
fn departure_point(bounds: &rataflow::Rect, side: HandlePosition, toward: Position) -> Position {
    let center = bounds.center();
    let (dx, dy) = (toward.x - center.x, toward.y - center.y);
    let inset = |value: f64, low: f64, high: f64| {
        if value.is_finite() {
            value.clamp(
                low + CORNER_INSET,
                (high - CORNER_INSET).max(low + CORNER_INSET),
            )
        } else {
            f64::midpoint(low, high)
        }
    };
    match side {
        HandlePosition::Left | HandlePosition::Right => {
            let y = dy.mul_add((bounds.width() / 2.0) / dx.abs(), center.y);
            let x = if side == HandlePosition::Left {
                bounds.x()
            } else {
                bounds.right()
            };
            Position::new(x, inset(y, bounds.y(), bounds.bottom()))
        }
        HandlePosition::Top | HandlePosition::Bottom => {
            let x = dx.mul_add((bounds.height() / 2.0) / dy.abs(), center.x);
            let y = if side == HandlePosition::Top {
                bounds.y()
            } else {
                bounds.bottom()
            };
            Position::new(inset(x, bounds.x(), bounds.right()), y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rataflow::Theme;

    fn render_small_table(name: &str, width: u16) -> String {
        let node = TableNode {
            name: name.to_string(),
            schema: Some("public".to_string()),
            self_reference_fks: vec![],
        };
        let area = Rect::new(0, 0, width, 2);
        let mut buf = Buffer::empty(area);
        node.render(
            &NodeRenderContext {
                id: name,
                area,
                selected: false,
                dragging: false,
                position_absolute: Position::new(0.0, 0.0),
                theme: Theme::default(),
                animation_phase: 0,
            },
            &mut buf,
        );
        (0..area.width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect()
    }

    #[test]
    fn zoomed_out_table_keeps_its_name_on_the_border() {
        assert_eq!(render_small_table("users", 12).trim_end(), "users");
    }

    #[test]
    fn zoomed_out_table_marks_a_cut_name() {
        assert_eq!(render_small_table("organizations", 6), "organ…");
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> rataflow::Rect {
        rataflow::Rect::from_coords(x, y, width, height)
    }

    fn fk_path(source: rataflow::Rect, target: rataflow::Rect) -> Path {
        FkEdge::default().compute_path(&EdgePathContext {
            from: source.center(),
            to: target.center(),
            source_position: HandlePosition::Right,
            target_position: HandlePosition::Left,
            source_bounds: source,
            target_bounds: Some(target),
        })
    }

    #[test]
    fn fks_into_one_table_share_the_arrowhead() {
        let target = rect(50.0, 10.0, 10.0, 3.0);

        let upper = fk_path(rect(0.0, 0.0, 10.0, 3.0), target);
        let lower = fk_path(rect(0.0, 20.0, 10.0, 3.0), target);

        assert_eq!(upper.end(), lower.end());
    }

    #[test]
    fn fks_out_of_one_side_leave_from_separate_points() {
        let source = rect(0.0, 20.0, 30.0, 3.0);

        let left = fk_path(source, rect(0.0, 0.0, 10.0, 3.0));
        let right = fk_path(source, rect(25.0, 0.0, 10.0, 3.0));

        assert_ne!(left.start(), right.start());
    }

    #[test]
    fn fk_never_leaves_from_a_corner() {
        let source = rect(0.0, 10.0, 10.0, 3.0);

        // Up and to the right: leaves the top side short of its right corner (x = 10).
        let path = fk_path(source, rect(30.0, 0.0, 10.0, 3.0));

        assert_eq!(path.start(), Some(Position::new(8.5, 10.0)));
    }

    #[test]
    fn fk_between_close_facing_sides_never_folds_back() {
        let source = rect(0.0, 0.0, 40.0, 3.0);

        // Wide tables two rows apart: they face each other bottom to top.
        let path = fk_path(source, rect(60.0, 5.0, 40.0, 3.0));

        let ys: Vec<f64> = path.points.iter().map(|point| point.y).collect();
        assert!(ys.windows(2).all(|pair| pair[0] <= pair[1]), "{ys:?}");
    }
}
