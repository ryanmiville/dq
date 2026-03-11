use anyhow::{Context, Result};
use duckdb::arrow::{
    datatypes::{DataType, TimeUnit},
    record_batch::RecordBatch,
    util::display::array_value_to_string,
};
use std::{
    env,
    io::{self, IsTerminal},
};
use terminal_size::{Width, terminal_size};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct DuckBox {
    config: Config,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Max render width. 0 = auto-detect terminal width.
    pub max_width: usize,
    /// Maximum rows to display before truncating. 0 = unlimited.
    pub max_rows: usize,
    /// Preferred maximum column width when shrinking wide tables.
    pub max_col_width: usize,
    /// String to display for NULL values.
    pub null_value: String,
    /// Whether to emit ANSI styling for borders/footer accents.
    pub color: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_width: parse_env_var("DQ_MAX_WIDTH").unwrap_or(0),
            max_rows: parse_env_var("DQ_MAX_ROWS").unwrap_or(20),
            max_col_width: parse_env_var("DQ_MAX_COL_WIDTH").unwrap_or(20),
            null_value: "NULL".to_string(),
            color: io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
        }
    }
}

impl DuckBox {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn render(&self, batches: &[RecordBatch]) -> Result<String> {
        if batches.is_empty() {
            return Ok(String::new());
        }

        let columns = extract_columns(&batches[0]);
        if columns.is_empty() {
            return Ok(String::new());
        }

        let rows = extract_rows(batches, &self.config.null_value)?;
        let selection = select_rows(&rows, self.config.max_rows);
        let natural_widths = compute_natural_widths(&columns, &selection);
        let max_width = resolved_max_width(self.config.max_width);
        let layout = fit_columns(
            &columns,
            &natural_widths,
            max_width,
            self.config.max_col_width.max(1),
        );
        let footer = build_footer(&selection, rows.len(), columns.len(), layout.hidden_columns);
        let style = RenderStyle::new(self.config.color);

        Ok(render_table(&layout, &selection, footer.as_ref(), &style))
    }
}

impl Default for DuckBox {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[derive(Clone, Debug)]
struct ColumnMeta {
    name: String,
    type_name: String,
    alignment: Alignment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Alignment {
    Left,
    Right,
    Center,
}

#[derive(Clone, Debug)]
struct LayoutColumn {
    source_index: Option<usize>,
    name: String,
    type_name: String,
    alignment: Alignment,
    width: usize,
}

#[derive(Clone, Debug)]
struct LayoutPlan {
    columns: Vec<LayoutColumn>,
    hidden_columns: usize,
}

#[derive(Clone, Debug)]
struct LayoutCandidate {
    hidden_range: (usize, usize),
    widths: Vec<usize>,
    total_width: usize,
    visible_source_columns: usize,
    left_visible_columns: usize,
    right_visible_columns: usize,
}

#[derive(Clone, Debug)]
struct Footer {
    left_primary: String,
    left_secondary: Option<String>,
    right_primary: String,
    right_secondary: Option<String>,
}

#[derive(Clone, Debug)]
struct RowSelection {
    rows: Vec<VisibleRow>,
    shown_count: usize,
    truncated: bool,
}

#[derive(Clone, Debug)]
enum VisibleRow {
    Data(Vec<String>),
    Divider,
}

const GRAY: &str = "\u{1b}[90m";

struct RenderStyle {
    border: &'static str,
    muted: &'static str,
    reset: &'static str,
}

impl RenderStyle {
    fn new(color: bool) -> Self {
        if color {
            Self {
                border: GRAY,
                muted: GRAY,
                reset: "\u{1b}[0m",
            }
        } else {
            Self {
                border: "",
                muted: "",
                reset: "",
            }
        }
    }

    fn border_text(&self, text: &str) -> String {
        format!("{}{}{}", self.border, text, self.reset)
    }

    fn muted_text(&self, text: &str) -> String {
        format!("{}{}{}", self.muted, text, self.reset)
    }
}

fn extract_columns(batch: &RecordBatch) -> Vec<ColumnMeta> {
    batch
        .schema()
        .fields()
        .iter()
        .map(|field| ColumnMeta {
            name: field.name().clone(),
            type_name: display_type_name(field.data_type()),
            alignment: alignment_for_type(field.data_type()),
        })
        .collect()
}

fn extract_rows(batches: &[RecordBatch], null_value: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();

    for batch in batches {
        for row_index in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(batch.num_columns());
            for column in batch.columns() {
                if column.is_null(row_index) {
                    row.push(null_value.to_string());
                } else {
                    row.push(
                        array_value_to_string(column.as_ref(), row_index)
                            .context("failed to stringify arrow value")?,
                    );
                }
            }
            rows.push(row);
        }
    }

    Ok(rows)
}

fn select_rows(rows: &[Vec<String>], max_rows: usize) -> RowSelection {
    if max_rows == 0 || rows.len() <= max_rows.saturating_add(3) || rows.len() <= max_rows {
        return RowSelection {
            rows: rows.iter().cloned().map(VisibleRow::Data).collect(),
            shown_count: rows.len(),
            truncated: false,
        };
    }

    let top_count = max_rows.div_ceil(2);
    let bottom_count = max_rows - top_count;
    let mut selected = Vec::with_capacity(max_rows + 3);

    for row in rows.iter().take(top_count) {
        selected.push(VisibleRow::Data(row.clone()));
    }
    selected.extend([
        VisibleRow::Divider,
        VisibleRow::Divider,
        VisibleRow::Divider,
    ]);
    for row in rows.iter().skip(rows.len() - bottom_count) {
        selected.push(VisibleRow::Data(row.clone()));
    }

    RowSelection {
        rows: selected,
        shown_count: max_rows,
        truncated: true,
    }
}

fn compute_natural_widths(columns: &[ColumnMeta], selection: &RowSelection) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut width = display_width(&column.name).max(display_width(&column.type_name));
            for row in &selection.rows {
                match row {
                    VisibleRow::Data(values) => {
                        width = width.max(display_width(&values[index]));
                    }
                    VisibleRow::Divider => {
                        width = width.max(1);
                    }
                }
            }
            width.max(1)
        })
        .collect()
}

fn fit_columns(
    columns: &[ColumnMeta],
    natural_widths: &[usize],
    max_width: usize,
    max_col_width: usize,
) -> LayoutPlan {
    if total_table_width(natural_widths) <= max_width {
        return build_layout(columns, natural_widths, None);
    }

    let preferred_widths = preferred_widths(natural_widths, max_col_width);

    if total_table_width(&preferred_widths) <= max_width {
        return build_layout(columns, &preferred_widths, None);
    }

    if let Some(candidate) = choose_pruned_candidate(&preferred_widths, max_width, false) {
        return build_layout(columns, &candidate.widths, Some(candidate.hidden_range));
    }

    if let Some(candidate) = choose_pruned_candidate(&preferred_widths, max_width, true) {
        return build_layout(columns, &candidate.widths, Some(candidate.hidden_range));
    }

    let mut widths = preferred_widths;
    shrink_widths(&mut widths, max_width, 3);
    if total_table_width(&widths) <= max_width {
        return build_layout(columns, &widths, None);
    }

    LayoutPlan {
        columns: columns
            .iter()
            .enumerate()
            .take(1)
            .map(|(index, column)| LayoutColumn {
                source_index: Some(index),
                name: column.name.clone(),
                type_name: column.type_name.clone(),
                alignment: column.alignment,
                width: widths[index],
            })
            .collect(),
        hidden_columns: columns.len().saturating_sub(1),
    }
}

fn preferred_widths(natural_widths: &[usize], max_col_width: usize) -> Vec<usize> {
    let max_col_width = max_col_width.max(3);
    natural_widths
        .iter()
        .map(|width| (*width).min(max_col_width))
        .collect()
}

fn choose_pruned_candidate(
    preferred_widths: &[usize],
    max_width: usize,
    allow_shrink: bool,
) -> Option<LayoutCandidate> {
    let mut best = None;

    for hidden_range in hidden_ranges(preferred_widths.len()) {
        let mut widths = preferred_widths.to_vec();
        if allow_shrink {
            shrink_widths_for_hidden_range(&mut widths, hidden_range, max_width, 3);
        }

        let total_width = total_width_for_hidden_range(&widths, hidden_range);
        if total_width > max_width {
            continue;
        }

        let candidate = LayoutCandidate {
            hidden_range,
            widths,
            total_width,
            visible_source_columns: preferred_widths.len() - hidden_range_width(hidden_range),
            left_visible_columns: hidden_range.0,
            right_visible_columns: preferred_widths.len() - hidden_range.1 - 1,
        };

        let replace = best
            .as_ref()
            .map(|current| is_better_candidate(&candidate, current))
            .unwrap_or(true);
        if replace {
            best = Some(candidate);
        }
    }

    best
}

fn is_better_candidate(candidate: &LayoutCandidate, current: &LayoutCandidate) -> bool {
    match candidate
        .visible_source_columns
        .cmp(&current.visible_source_columns)
    {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }

    let candidate_balance = candidate
        .left_visible_columns
        .min(candidate.right_visible_columns);
    let current_balance = current
        .left_visible_columns
        .min(current.right_visible_columns);
    match candidate_balance.cmp(&current_balance) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }

    let candidate_imbalance = candidate
        .left_visible_columns
        .abs_diff(candidate.right_visible_columns);
    let current_imbalance = current
        .left_visible_columns
        .abs_diff(current.right_visible_columns);
    match candidate_imbalance.cmp(&current_imbalance) {
        std::cmp::Ordering::Less => return true,
        std::cmp::Ordering::Greater => return false,
        std::cmp::Ordering::Equal => {}
    }

    match candidate.total_width.cmp(&current.total_width) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => candidate.hidden_range.0 < current.hidden_range.0,
    }
}

fn build_layout(
    columns: &[ColumnMeta],
    widths: &[usize],
    hidden_range: Option<(usize, usize)>,
) -> LayoutPlan {
    let Some((hidden_start, hidden_end)) = hidden_range else {
        return LayoutPlan {
            columns: columns
                .iter()
                .enumerate()
                .map(|(index, column)| LayoutColumn {
                    source_index: Some(index),
                    name: column.name.clone(),
                    type_name: column.type_name.clone(),
                    alignment: column.alignment,
                    width: widths[index],
                })
                .collect(),
            hidden_columns: 0,
        };
    };

    let hidden_count = hidden_range_width((hidden_start, hidden_end));
    let mut layout = Vec::with_capacity(columns.len() - hidden_count + 1);
    for (index, column) in columns.iter().enumerate() {
        if index == hidden_start {
            layout.push(LayoutColumn {
                source_index: None,
                name: "…".to_string(),
                type_name: String::new(),
                alignment: Alignment::Center,
                width: 1,
            });
        }

        if (hidden_start..=hidden_end).contains(&index) {
            continue;
        }

        layout.push(LayoutColumn {
            source_index: Some(index),
            name: column.name.clone(),
            type_name: column.type_name.clone(),
            alignment: column.alignment,
            width: widths[index],
        });
    }

    LayoutPlan {
        columns: layout,
        hidden_columns: hidden_count,
    }
}

fn hidden_ranges(len: usize) -> Vec<(usize, usize)> {
    if len < 3 {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    for start in 1..len - 1 {
        for end in start..=len - 2 {
            ranges.push((start, end));
        }
    }
    ranges
}

fn hidden_range_width(hidden_range: (usize, usize)) -> usize {
    hidden_range.1 - hidden_range.0 + 1
}

fn total_width_for_hidden_range(widths: &[usize], hidden_range: (usize, usize)) -> usize {
    let (hidden_start, hidden_end) = hidden_range;
    let mut layout_widths = Vec::with_capacity(widths.len() - hidden_range_width(hidden_range) + 1);
    layout_widths.extend_from_slice(&widths[..hidden_start]);
    layout_widths.push(1);
    layout_widths.extend_from_slice(&widths[hidden_end + 1..]);
    total_table_width(&layout_widths)
}

fn shrink_widths_for_hidden_range(
    widths: &mut [usize],
    hidden_range: (usize, usize),
    max_width: usize,
    min_width: usize,
) {
    let visible_indices = (0..hidden_range.0)
        .chain(hidden_range.1 + 1..widths.len())
        .collect::<Vec<_>>();
    let mut current_width = total_width_for_hidden_range(widths, hidden_range);

    while current_width > max_width {
        let Some(max_seen) = visible_indices.iter().map(|index| widths[*index]).max() else {
            return;
        };
        if max_seen <= min_width {
            return;
        }

        let mut changed = false;
        let widest_indices = visible_indices
            .iter()
            .copied()
            .filter(|index| widths[*index] == max_seen)
            .collect::<Vec<_>>();
        for index in widest_indices {
            if current_width <= max_width {
                break;
            }
            widths[index] -= 1;
            current_width -= 1;
            changed = true;
        }

        if !changed {
            return;
        }
    }
}

fn shrink_widths(widths: &mut [usize], max_width: usize, min_width: usize) {
    let mut current_width = total_table_width(widths);

    while current_width > max_width {
        let Some(max_seen) = widths.iter().copied().max() else {
            return;
        };
        if max_seen <= min_width {
            return;
        }

        let mut changed = false;
        for width in widths.iter_mut().filter(|width| **width == max_seen) {
            if current_width <= max_width {
                break;
            }
            *width -= 1;
            current_width -= 1;
            changed = true;
        }

        if !changed {
            return;
        }
    }
}

fn build_footer(
    selection: &RowSelection,
    total_rows: usize,
    total_columns: usize,
    hidden_columns: usize,
) -> Option<Footer> {
    if !selection.truncated && hidden_columns == 0 {
        return None;
    }

    let visible_columns = total_columns.saturating_sub(hidden_columns);

    Some(Footer {
        left_primary: count_phrase(total_rows, "row"),
        left_secondary: selection
            .truncated
            .then(|| format!("{} shown", selection.shown_count)),
        right_primary: count_phrase(total_columns, "column"),
        right_secondary: (hidden_columns > 0).then(|| format!("{} shown", visible_columns)),
    })
}

fn count_phrase(count: usize, singular: &str) -> String {
    let plural = if count == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    };
    format!("{count} {plural}")
}

fn render_table(
    layout: &LayoutPlan,
    selection: &RowSelection,
    footer: Option<&Footer>,
    style: &RenderStyle,
) -> String {
    let widths: Vec<_> = layout.columns.iter().map(|column| column.width).collect();
    let mut lines = Vec::new();

    lines.push(render_segmented_border(&widths, '┌', '┬', '┐', style));
    lines.push(render_column_row(
        &layout.columns,
        |column| column.name.as_str(),
        Alignment::Center,
        style,
    ));
    lines.push(render_column_row(
        &layout.columns,
        |column| column.type_name.as_str(),
        Alignment::Center,
        style,
    ));
    lines.push(render_segmented_border(&widths, '├', '┼', '┤', style));

    let mut last_data_row: Option<&[String]> = None;
    for row in &selection.rows {
        match row {
            VisibleRow::Data(values) => {
                lines.push(render_data_row(&layout.columns, values, style));
                last_data_row = Some(values);
            }
            VisibleRow::Divider => {
                lines.push(render_divider_row(&layout.columns, last_data_row, style));
            }
        }
    }

    if let Some(footer) = footer {
        let inner_width = total_layout_width(&layout.columns).saturating_sub(2);
        lines.push(render_segmented_border(&widths, '├', '┴', '┤', style));
        lines.extend(render_footer_lines(inner_width, footer, style));
        lines.push(render_full_border(inner_width, '└', '┘', style));
    } else {
        lines.push(render_segmented_border(&widths, '└', '┴', '┘', style));
    }

    lines.join("\n")
}

fn render_column_row(
    columns: &[LayoutColumn],
    value_for: impl Fn(&LayoutColumn) -> &str,
    alignment: Alignment,
    style: &RenderStyle,
) -> String {
    let mut line = style.border_text("│");
    for column in columns {
        line.push_str(&render_text_styled(
            value_for(column),
            column.width,
            alignment,
            column.source_index.is_none(),
            style,
        ));
        line.push_str(&style.border_text("│"));
    }
    line
}

fn render_data_row(columns: &[LayoutColumn], values: &[String], style: &RenderStyle) -> String {
    let mut line = style.border_text("│");
    for column in columns {
        let value = column
            .source_index
            .map(|index| values[index].as_str())
            .unwrap_or("…");
        line.push_str(&render_text_styled(
            value,
            column.width,
            column.alignment,
            column.source_index.is_none(),
            style,
        ));
        line.push_str(&style.border_text("│"));
    }
    line
}

fn render_divider_row(
    columns: &[LayoutColumn],
    reference_values: Option<&[String]>,
    style: &RenderStyle,
) -> String {
    let mut line = style.border_text("│");
    for column in columns {
        let reference_value = column
            .source_index
            .and_then(|index| reference_values.and_then(|values| values.get(index)))
            .map(|value| truncate_to_width(value, column.width));
        line.push_str(&render_divider_text(
            column.width,
            column.alignment,
            reference_value.as_deref(),
            style,
        ));
        line.push_str(&style.border_text("│"));
    }
    line
}

fn render_divider_text(
    width: usize,
    alignment: Alignment,
    reference_value: Option<&str>,
    style: &RenderStyle,
) -> String {
    let dot = style.border_text("·");

    if let Some(reference_value) = reference_value {
        let actual_width = display_width(reference_value);
        if actual_width > 0 {
            let start = match alignment {
                Alignment::Left => 0,
                Alignment::Right => width.saturating_sub(actual_width),
                Alignment::Center => width.saturating_sub(actual_width) / 2,
            };
            let dot_offset = start + (actual_width - 1) / 2;
            let right_padding = width.saturating_sub(dot_offset + 1);
            return format!(
                " {}{}{} ",
                " ".repeat(dot_offset),
                dot,
                " ".repeat(right_padding)
            );
        }
    }

    format!(
        " {} ",
        pad("·", width, divider_alignment(alignment)).replacen('·', &dot, 1)
    )
}

fn divider_alignment(alignment: Alignment) -> Alignment {
    match alignment {
        Alignment::Right => Alignment::Right,
        Alignment::Left | Alignment::Center => Alignment::Center,
    }
}

fn render_footer_lines(inner_width: usize, footer: &Footer, style: &RenderStyle) -> Vec<String> {
    let left_plain = footer_plain_side(&footer.left_primary, footer.left_secondary.as_deref());
    let right_plain = footer_plain_side(&footer.right_primary, footer.right_secondary.as_deref());

    if inner_width >= 2
        && display_width(&left_plain) + display_width(&right_plain) <= inner_width - 2
    {
        let left = render_footer_side(
            &footer.left_primary,
            footer.left_secondary.as_deref(),
            style,
        );
        let right = render_footer_side(
            &footer.right_primary,
            footer.right_secondary.as_deref(),
            style,
        );
        let spaces = inner_width - 2 - display_width(&left_plain) - display_width(&right_plain);
        return vec![format!(
            "{} {}{}{} {}",
            style.border_text("│"),
            left,
            " ".repeat(spaces),
            right,
            style.border_text("│")
        )];
    }

    let grouped_lines = [
        footer_plain_side(&footer.left_primary, footer.left_secondary.as_deref()),
        footer_plain_side(&footer.right_primary, footer.right_secondary.as_deref()),
    ];
    if grouped_lines
        .iter()
        .all(|line| display_width(line) <= inner_width)
    {
        return grouped_lines
            .into_iter()
            .map(|line| render_footer_centered_line(inner_width, &line, style))
            .collect();
    }

    let mut lines = Vec::new();
    lines.push(render_footer_centered_line(
        inner_width,
        &footer.left_primary,
        style,
    ));
    if let Some(left_secondary) = &footer.left_secondary {
        lines.push(render_footer_centered_line(
            inner_width,
            left_secondary,
            style,
        ));
    }
    lines.push(render_footer_centered_line(
        inner_width,
        &footer.right_primary,
        style,
    ));
    if let Some(right_secondary) = &footer.right_secondary {
        lines.push(render_footer_centered_line(
            inner_width,
            right_secondary,
            style,
        ));
    }
    lines
}

fn render_footer_centered_line(inner_width: usize, value: &str, style: &RenderStyle) -> String {
    let value = truncate_to_width(value, inner_width);
    format!(
        "{}{}{}",
        style.border_text("│"),
        pad(&value, inner_width, Alignment::Center),
        style.border_text("│")
    )
}

fn render_footer_side(primary: &str, secondary: Option<&str>, style: &RenderStyle) -> String {
    match secondary {
        Some(secondary) => format!("{primary} {}", style.muted_text(&format!("({secondary})"))),
        None => primary.to_string(),
    }
}

fn footer_plain_side(primary: &str, secondary: Option<&str>) -> String {
    match secondary {
        Some(secondary) => format!("{primary} ({secondary})"),
        None => primary.to_string(),
    }
}

fn render_segmented_border(
    widths: &[usize],
    left: char,
    mid: char,
    right: char,
    style: &RenderStyle,
) -> String {
    let segments = widths
        .iter()
        .map(|width| "─".repeat(width + 2))
        .collect::<Vec<_>>()
        .join(&mid.to_string());
    style.border_text(&format!("{left}{segments}{right}"))
}

fn render_full_border(inner_width: usize, left: char, right: char, style: &RenderStyle) -> String {
    style.border_text(&format!("{left}{}{right}", "─".repeat(inner_width)))
}

fn render_text(value: &str, width: usize, alignment: Alignment) -> String {
    let truncated = truncate_to_width(value, width);
    format!(" {} ", pad(&truncated, width, alignment))
}

fn render_text_styled(
    value: &str,
    width: usize,
    alignment: Alignment,
    muted: bool,
    style: &RenderStyle,
) -> String {
    let truncated = truncate_to_width(value, width);
    if !muted {
        return render_text(&truncated, width, alignment);
    }

    let actual_width = display_width(&truncated);
    let padding = width.saturating_sub(actual_width);
    let (left, right) = match alignment {
        Alignment::Left => (0, padding),
        Alignment::Right => (padding, 0),
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            (left, right)
        }
    };

    format!(
        " {}{}{} ",
        " ".repeat(left),
        style.muted_text(&truncated),
        " ".repeat(right)
    )
}

fn pad(value: &str, width: usize, alignment: Alignment) -> String {
    let actual_width = display_width(value);
    let padding = width.saturating_sub(actual_width);

    match alignment {
        Alignment::Left => format!("{value}{}", " ".repeat(padding)),
        Alignment::Right => format!("{}{value}", " ".repeat(padding)),
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
        }
    }
}

fn truncate_to_width(value: &str, max_width: usize) -> String {
    if display_width(value) <= max_width {
        return value.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut width = 0;

    for ch in value.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width + 1 > max_width {
            break;
        }
        out.push(ch);
        width += ch_width;
    }

    out.push('…');
    out
}

fn total_table_width(widths: &[usize]) -> usize {
    1 + widths.iter().map(|width| width + 3).sum::<usize>()
}

fn total_layout_width(columns: &[LayoutColumn]) -> usize {
    total_table_width(
        &columns
            .iter()
            .map(|column| column.width)
            .collect::<Vec<_>>(),
    )
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn resolved_max_width(configured: usize) -> usize {
    if configured > 0 {
        return configured;
    }

    terminal_size()
        .map(|(Width(width), _)| usize::from(width))
        .unwrap_or(120)
}

fn alignment_for_type(data_type: &DataType) -> Alignment {
    match data_type {
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float16
        | DataType::Float32
        | DataType::Float64
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _) => Alignment::Right,
        DataType::Dictionary(_, value_type) => alignment_for_type(value_type),
        _ => Alignment::Left,
    }
}

fn display_type_name(data_type: &DataType) -> String {
    match data_type {
        DataType::Null => "null".to_string(),
        DataType::Boolean => "boolean".to_string(),
        DataType::Int8 => "tinyint".to_string(),
        DataType::Int16 => "smallint".to_string(),
        DataType::Int32 => "integer".to_string(),
        DataType::Int64 => "bigint".to_string(),
        DataType::UInt8 => "utinyint".to_string(),
        DataType::UInt16 => "usmallint".to_string(),
        DataType::UInt32 => "uinteger".to_string(),
        DataType::UInt64 => "ubigint".to_string(),
        DataType::Float16 | DataType::Float32 => "float".to_string(),
        DataType::Float64 => "double".to_string(),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "varchar".to_string(),
        DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_) => "blob".to_string(),
        DataType::Date32 | DataType::Date64 => "date".to_string(),
        DataType::Time32(_) | DataType::Time64(_) => "time".to_string(),
        DataType::Duration(_) | DataType::Interval(_) => "interval".to_string(),
        DataType::Timestamp(time_unit, _) => match time_unit {
            TimeUnit::Second => "timestamp_s".to_string(),
            TimeUnit::Millisecond => "timestamp_ms".to_string(),
            TimeUnit::Microsecond => "timestamp".to_string(),
            TimeUnit::Nanosecond => "timestamp_ns".to_string(),
        },
        DataType::Decimal128(precision, scale) => format!("decimal({precision},{scale})"),
        DataType::Decimal256(precision, scale) => format!("decimal({precision},{scale})"),
        DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _) => {
            format!("{}[]", display_type_name(field.data_type()))
        }
        DataType::Dictionary(_, value_type) => display_type_name(value_type),
        DataType::Struct(_) => "struct".to_string(),
        _ => "varchar".to_string(),
    }
}

fn parse_env_var(var_name: &str) -> Option<usize> {
    env::var(var_name).ok().and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::arrow::{
        array::{ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };
    use indoc::indoc;
    use std::sync::Arc;

    fn batch(fields: Vec<Field>, columns: Vec<ArrayRef>) -> RecordBatch {
        RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).unwrap()
    }

    #[test]
    fn basic_render() {
        let batch = batch(
            vec![
                Field::new("name", DataType::Utf8, false),
                Field::new("age", DataType::Int64, false),
            ],
            vec![
                Arc::new(StringArray::from(vec!["Ada", "Linus"])) as ArrayRef,
                Arc::new(Int64Array::from(vec![37, 54])) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 80,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        assert_eq!(
            table,
            indoc! {"
                ┌─────────┬────────┐
                │  name   │  age   │
                │ varchar │ bigint │
                ├─────────┼────────┤
                │ Ada     │     37 │
                │ Linus   │     54 │
                └─────────┴────────┘"
            }
        );
    }

    #[test]
    fn renders_nulls_and_utf8_widths() {
        let batch = batch(
            vec![
                Field::new("emoji", DataType::Utf8, true),
                Field::new("active", DataType::Boolean, true),
            ],
            vec![
                Arc::new(StringArray::from(vec![Some("🦆"), None])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![Some(true), None])) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 80,
            null_value: "NULL".to_string(),
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        let expected = indoc! {"
            ┌─────────┬─────────┐
            │  emoji  │ active  │
            │ varchar │ boolean │
            ├─────────┼─────────┤
            │ 🦆      │ true    │
            │ NULL    │ NULL    │
            └─────────┴─────────┘"
        };
        assert_eq!(table, expected);
    }

    #[test]
    fn renders_mixed_scalar_columns_exactly() {
        let batch = batch(
            vec![
                Field::new("id", DataType::Int64, false),
                Field::new("score", DataType::Float64, false),
                Field::new("active", DataType::Boolean, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
                Arc::new(Float64Array::from(vec![1.25, 0.0])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![false, true])) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 80,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        let expected = indoc! {"
            ┌────────┬────────┬─────────┐
            │   id   │ score  │ active  │
            │ bigint │ double │ boolean │
            ├────────┼────────┼─────────┤
            │      1 │   1.25 │ false   │
            │      2 │    0.0 │ true    │
            └────────┴────────┴─────────┘"
        };
        assert_eq!(table, expected);
    }

    #[test]
    fn row_truncation_renders_dividers_and_footer() {
        let values = (1..=8).map(|value| value.to_string()).collect::<Vec<_>>();
        let refs = values
            .iter()
            .map(|value| Some(value.as_str()))
            .collect::<Vec<_>>();
        let batch = batch(
            vec![Field::new("value", DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(refs)) as ArrayRef],
        );

        let table = DuckBox::new(Config {
            max_width: 80,
            max_rows: 4,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        let expected = indoc! {"
            ┌─────────┐
            │  value  │
            │ varchar │
            ├─────────┤
            │ 1       │
            │ 2       │
            │ ·       │
            │ ·       │
            │ ·       │
            │ 7       │
            │ 8       │
            ├─────────┤
            │ 8 rows  │
            │ 4 shown │
            │1 column │
            └─────────┘"
        };
        assert_eq!(table, expected);
    }

    #[test]
    fn divider_rows_follow_reference_value_positioning() {
        let row = render_divider_row(
            &[
                LayoutColumn {
                    source_index: Some(0),
                    name: "dept".to_string(),
                    type_name: "varchar".to_string(),
                    alignment: Alignment::Left,
                    width: 11,
                },
                LayoutColumn {
                    source_index: Some(1),
                    name: "n".to_string(),
                    type_name: "bigint".to_string(),
                    alignment: Alignment::Right,
                    width: 6,
                },
                LayoutColumn {
                    source_index: Some(2),
                    name: "active".to_string(),
                    type_name: "boolean".to_string(),
                    alignment: Alignment::Left,
                    width: 7,
                },
                LayoutColumn {
                    source_index: None,
                    name: "…".to_string(),
                    type_name: "…".to_string(),
                    alignment: Alignment::Center,
                    width: 3,
                },
            ],
            Some(&["Sales".to_string(), "10".to_string(), "true".to_string()]),
            &RenderStyle::new(false),
        );

        assert_eq!(row, "│   ·         │     ·  │  ·      │  ·  │");
    }

    #[test]
    fn footer_uses_single_line_when_wide_enough() {
        let batch = batch(
            vec![
                Field::new("name", DataType::Utf8, false),
                Field::new("dept", DataType::Utf8, false),
                Field::new("n", DataType::Int64, false),
            ],
            vec![
                Arc::new(StringArray::from(
                    (1..=24)
                        .map(|i| format!("Employee_{i:03}"))
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(vec!["Ops"; 24])) as ArrayRef,
                Arc::new(Int64Array::from((1..=24).collect::<Vec<_>>())) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 120,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        assert!(
            table.contains("│ 24 rows (20 shown)    3 columns │"),
            "{table}"
        );
    }

    #[test]
    fn row_truncation_three_column_snapshot_matches_fixture_coverage() {
        let batch = batch(
            vec![
                Field::new("name", DataType::Utf8, false),
                Field::new("dept", DataType::Utf8, false),
                Field::new("n", DataType::Int64, false),
            ],
            vec![
                Arc::new(StringArray::from(
                    (1..=24)
                        .map(|i| format!("Employee_{i:03}"))
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(vec!["Ops"; 24])) as ArrayRef,
                Arc::new(Int64Array::from((1..=24).collect::<Vec<_>>())) as ArrayRef,
            ],
        );

        let actual = DuckBox::new(Config {
            max_width: 120,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        let expected = indoc! {"
            ┌──────────────┬─────────┬────────┐
            │     name     │  dept   │   n    │
            │   varchar    │ varchar │ bigint │
            ├──────────────┼─────────┼────────┤
            │ Employee_001 │ Ops     │      1 │
            │ Employee_002 │ Ops     │      2 │
            │ Employee_003 │ Ops     │      3 │
            │ Employee_004 │ Ops     │      4 │
            │ Employee_005 │ Ops     │      5 │
            │ Employee_006 │ Ops     │      6 │
            │ Employee_007 │ Ops     │      7 │
            │ Employee_008 │ Ops     │      8 │
            │ Employee_009 │ Ops     │      9 │
            │ Employee_010 │ Ops     │     10 │
            │      ·       │  ·      │     ·  │
            │      ·       │  ·      │     ·  │
            │      ·       │  ·      │     ·  │
            │ Employee_015 │ Ops     │     15 │
            │ Employee_016 │ Ops     │     16 │
            │ Employee_017 │ Ops     │     17 │
            │ Employee_018 │ Ops     │     18 │
            │ Employee_019 │ Ops     │     19 │
            │ Employee_020 │ Ops     │     20 │
            │ Employee_021 │ Ops     │     21 │
            │ Employee_022 │ Ops     │     22 │
            │ Employee_023 │ Ops     │     23 │
            │ Employee_024 │ Ops     │     24 │
            ├──────────────┴─────────┴────────┤
            │ 24 rows (20 shown)    3 columns │
            └─────────────────────────────────┘"
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn colorized_output_wraps_borders_in_ansi_sequences() {
        let batch = batch(
            vec![Field::new("name", DataType::Utf8, false)],
            vec![Arc::new(StringArray::from(vec!["Ada"])) as ArrayRef],
        );

        let table = DuckBox::new(Config {
            max_width: 80,
            color: true,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        let p = format!("{}┌", GRAY);
        assert!(table.contains(p.as_str()), "{table:?}");
        assert!(table.contains("Ada"), "{table:?}");
    }

    #[test]
    fn colorized_truncation_dots_use_border_styling() {
        let actual = render_divider_row(
            &[
                LayoutColumn {
                    source_index: Some(0),
                    name: "dept".to_string(),
                    type_name: "varchar".to_string(),
                    alignment: Alignment::Left,
                    width: 11,
                },
                LayoutColumn {
                    source_index: Some(1),
                    name: "n".to_string(),
                    type_name: "bigint".to_string(),
                    alignment: Alignment::Right,
                    width: 6,
                },
            ],
            Some(&["Sales".to_string(), "10".to_string()]),
            &RenderStyle::new(true),
        );

        let border = GRAY;
        let reset = "\u{1b}[0m";
        let dot = format!("{border}·{reset}");

        let expected = format!(
            "{border}│{reset}   {dot}         {border}│{reset}     {dot}  {border}│{reset}"
        );

        assert_eq!(actual.matches(&dot).count(), 2, "{actual:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn colorized_hidden_column_ellipsis_uses_muted_styling() {
        let actual = render_data_row(
            &[
                LayoutColumn {
                    source_index: Some(0),
                    name: "name".to_string(),
                    type_name: "varchar".to_string(),
                    alignment: Alignment::Left,
                    width: 2,
                },
                LayoutColumn {
                    source_index: None,
                    name: "…".to_string(),
                    type_name: "…".to_string(),
                    alignment: Alignment::Center,
                    width: 1,
                },
            ],
            &["ok".to_string()],
            &RenderStyle::new(true),
        );

        let border = GRAY;
        let reset = "\u{1b}[0m";
        let muted = format!("{border}…{reset}");
        let expected = format!("{border}│{reset} ok {border}│{reset} {muted} {border}│{reset}");

        assert!(actual.contains(&muted), "{actual:?}");
        assert_eq!(actual, expected);
    }

    #[test]
    fn hidden_column_placeholder_type_row_is_blank() {
        let layout = build_layout(
            &[
                ColumnMeta {
                    name: "a".to_string(),
                    type_name: "x".to_string(),
                    alignment: Alignment::Left,
                },
                ColumnMeta {
                    name: "b".to_string(),
                    type_name: "y".to_string(),
                    alignment: Alignment::Left,
                },
                ColumnMeta {
                    name: "c".to_string(),
                    type_name: "z".to_string(),
                    alignment: Alignment::Left,
                },
            ],
            &[1, 1, 1],
            Some((1, 1)),
        );
        let style = RenderStyle::new(false);

        assert_eq!(layout.columns[1].name, "…");
        assert_eq!(layout.columns[1].type_name, "");
        assert_eq!(
            render_column_row(
                &layout.columns,
                |column| column.name.as_str(),
                Alignment::Center,
                &style,
            ),
            "│ a │ … │ c │"
        );
        assert_eq!(
            render_column_row(
                &layout.columns,
                |column| column.type_name.as_str(),
                Alignment::Center,
                &style,
            ),
            "│ x │   │ z │"
        );
        assert_eq!(
            render_data_row(
                &layout.columns,
                &["1".to_string(), "2".to_string(), "3".to_string()],
                &style,
            ),
            "│ 1 │ … │ 3 │"
        );
    }

    #[test]
    fn max_width_prefers_omitting_middle_columns_over_truncating_everything() {
        let batch = batch(
            vec![
                Field::new("id", DataType::Int64, false),
                Field::new("name", DataType::Utf8, false),
                Field::new("department", DataType::Utf8, false),
                Field::new("title", DataType::Utf8, false),
                Field::new("city", DataType::Utf8, false),
                Field::new("salary", DataType::Float64, false),
                Field::new("years_exp", DataType::Int64, false),
                Field::new("active", DataType::Boolean, false),
                Field::new("rating", DataType::Float64, false),
            ],
            vec![
                Arc::new(Int64Array::from(vec![1])) as ArrayRef,
                Arc::new(StringArray::from(vec!["Employee_001"])) as ArrayRef,
                Arc::new(StringArray::from(vec!["Marketing"])) as ArrayRef,
                Arc::new(StringArray::from(vec!["Engineer"])) as ArrayRef,
                Arc::new(StringArray::from(vec!["Seattle"])) as ArrayRef,
                Arc::new(Float64Array::from(vec![81730.0])) as ArrayRef,
                Arc::new(Int64Array::from(vec![4])) as ArrayRef,
                Arc::new(BooleanArray::from(vec![true])) as ArrayRef,
                Arc::new(Float64Array::from(vec![3.7])) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 75,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        assert!(table.contains("department"), "{table}");
        assert!(table.contains("years_exp"), "{table}");
        assert!(
            table.contains("│ … │") || table.contains("│  …  │"),
            "{table}"
        );
        assert!(table.contains("9 columns"), "{table}");
        assert!(table.contains("6 shown"), "{table}");
        assert!(!table.contains("depa…"), "{table}");
        assert!(!table.contains("year…"), "{table}");
    }

    #[test]
    fn truncates_wide_values_and_prunes_columns() {
        let batch = batch(
            vec![
                Field::new("first", DataType::Utf8, false),
                Field::new("second", DataType::Utf8, false),
                Field::new("third", DataType::Float64, false),
                Field::new("fourth", DataType::Utf8, false),
            ],
            vec![
                Arc::new(StringArray::from(vec!["supercalifragilistic"])) as ArrayRef,
                Arc::new(StringArray::from(vec!["middle"])) as ArrayRef,
                Arc::new(Float64Array::from(vec![1234.5])) as ArrayRef,
                Arc::new(StringArray::from(vec!["tail"])) as ArrayRef,
            ],
        );

        let table = DuckBox::new(Config {
            max_width: 24,
            max_col_width: 8,
            color: false,
            ..Config::default()
        })
        .render(&[batch])
        .unwrap();

        assert!(table.contains("su…"), "{table}");
        assert!(
            table.contains("│ … │") || table.contains("│  …  │"),
            "{table}"
        );
        assert!(table.contains("4 columns (3 shown)"), "{table}");
    }

    #[test]
    fn type_display_mapping_prefers_duckdb_names() {
        assert_eq!(display_type_name(&DataType::Int64), "bigint");
        assert_eq!(display_type_name(&DataType::Float64), "double");
        assert_eq!(display_type_name(&DataType::Boolean), "boolean");
        assert_eq!(display_type_name(&DataType::Utf8), "varchar");
        assert_eq!(
            display_type_name(&DataType::Decimal128(10, 2)),
            "decimal(10,2)"
        );
    }
}
