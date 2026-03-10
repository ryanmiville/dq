# Plan: DuckBox Renderer in Rust

Reimplement DuckDB's box-drawing table renderer ("duckbox") as a pure-Rust module
inside `dq`, replacing the current Arrow `pretty_format_batches` output with the
same Unicode box-drawing format used by the DuckDB CLI and Python client.

## Motivation

`dq to pretty` and the auto-pretty TTY mode currently produce Arrow's ASCII table
format (`+`/`-`/`|`). DuckDB's native renderer is strictly better:

- Unicode box-drawing characters (`┌─┬─┐`, `│`, `└─┴─┘`)
- Type annotations under column names
- Right-aligned numbers, left-aligned strings
- Smart row truncation with `· · ·` divider
- Column pruning when the table is too wide for the terminal
- Row count / column count footer

**Current output:**
```
+-------+-----+
| name  | age |
+-------+-----+
| Ada   | 37  |
| Linus | 54  |
+-------+-----+
```

**Target output:**
```
┌─────────┬───────┐
│  name   │  age  │
│ varchar │ int64 │
├─────────┼───────┤
│ Ada     │    37 │
│ Linus   │    54 │
└─────────┴───────┘
```

---

## Reference

The C++ implementation lives in two files:

- **Header:** `src/include/duckdb/common/box_renderer.hpp` (~180 lines)
- **Implementation:** `src/common/box_renderer.cpp` (~2,300 lines)

Source: <https://github.com/duckdb/duckdb/blob/main/src/common/box_renderer.cpp>

Much of the C++ code is plumbing for DuckDB's internal `ColumnDataCollection` /
`Vector` / `DataChunk` types. Our input is Arrow `RecordBatch` slices already
materialized in memory, so the data-access layer is trivial.

---

## Scope

### In scope (v1)

| Feature | DuckDB equivalent |
|---------|-------------------|
| Unicode box-drawing borders | `┌┬┐├┼┤└┴┘│─` |
| Two-row header: column name + type | Centered, type row below name row |
| Value alignment | Right-align numbers, left-align strings/booleans, center header |
| NULL rendering | Configurable null placeholder (default `"NULL"`) |
| UTF-8 aware column widths | Grapheme-cluster render width via `unicode-width` |
| Value truncation | Truncate with `…` when value exceeds column width |
| Row truncation | Top N/2 + bottom N/2 with `· · ·` divider rows |
| Footer | Row count, "(N shown)" when truncated, column count |
| Column pruning | When total width > terminal, prune from middle outward with `…` column |
| Column shrinking | Shorten wide columns equally before resorting to pruning |
| Terminal width detection | Auto-detect via `terminal_size`; fall back to 120 |
| Configurable max rows | Default 20 (matches DuckDB) |

### Out of scope (future / never)

| Feature | Reason |
|---------|--------|
| Column-mode pivot (`.columns`) | Niche, not needed for `dq` |
| ANSI syntax highlighting | Adds complexity; can layer on later |
| Row expansion / multi-line wrapping | Nice-to-have, not v1 |
| Large number formatting ("1.23 million") | Nice-to-have, not v1 |
| Thousand / decimal separator config | Nice-to-have, not v1 |

---

## Architecture

### New file: `src/duckbox.rs`

A single self-contained module. No new crate — this is internal to `dq`.

```
src/
├── main.rs
├── cmd.rs
├── format.rs
└── duckbox.rs    ← new
```

### Public API

```rust
use duckdb::arrow::record_batch::RecordBatch;

pub struct DuckBox {
    config: Config,
}

pub struct Config {
    /// Max render width. 0 = auto-detect terminal width.
    pub max_width: usize,
    /// Maximum rows to display before truncating.
    pub max_rows: usize,
    /// Maximum column width (only applied when table doesn't fit).
    pub max_col_width: usize,
    /// String to display for NULL values.
    pub null_value: String,
}

impl Default for Config { /* max_width: 0, max_rows: 20, max_col_width: 20, null_value: "NULL" */ }

impl DuckBox {
    pub fn new(config: Config) -> Self;

    /// Render record batches to a String.
    pub fn render(&self, batches: &[RecordBatch]) -> String;
}
```

### Integration point

In `cmd.rs`, replace `pretty_format_batches(&batches).to_string()` with
`DuckBox::new(Config::default()).render(&batches)`.

The change is exactly one call site — `pretty_query()`:

```rust
// Before
let table = pretty_format_batches(&batches)?;

// After
let table = DuckBox::new(Config::default()).render(&batches);
```

---

## Detailed design

### 1. Data extraction from Arrow

Extract column metadata and cell values from `&[RecordBatch]`:

```rust
struct ColumnMeta {
    name: String,       // from schema field name
    type_name: String,  // DuckDB-style type string (see type mapping below)
    alignment: Alignment,
}

enum Alignment { Left, Right, Center }
```

**Type name mapping** (Arrow `DataType` → display string):

| Arrow DataType | Display |
|----------------|---------|
| `Int8/16/32/64` | `tinyint`, `smallint`, `integer`, `bigint` |
| `UInt8/16/32/64` | `utinyint`, `usmallint`, `uinteger`, `ubigint` |
| `Float32/64` | `float`, `double` |
| `Boolean` | `boolean` |
| `Utf8/LargeUtf8` | `varchar` |
| `Date32/Date64` | `date` |
| `Timestamp(*, _)` | `timestamp` / `timestamp_s` / `timestamp_ms` / `timestamp_ns` |
| `Time32/Time64` | `time` |
| `Duration` | `interval` |
| `Decimal128(p,s)` | `decimal(p,s)` |
| `Binary/LargeBinary` | `blob` |
| `List(inner)` | `{inner_type}[]` |
| `Struct(fields)` | `struct(...)` |
| `Null` | `null` |
| Other | `varchar` (fallback) |

**Alignment rules:**

- Right-align: all integer and floating-point types, decimals
- Left-align: everything else (varchar, boolean, date, timestamp, etc.)
- Center: column names and type names in the header

**Cell value stringification:**

Use Arrow's built-in `array.value_as_string(row)` or the `display` formatter
from `arrow::util::display`. NULL values become the configured `null_value` string.

### 2. Row selection (top/bottom split)

```
total_rows = sum of batch row counts
rows_to_render = min(total_rows, max_rows)

if total_rows <= max_rows + 3:
    // hiding adds 3 lines (· · ·), so don't bother if close
    render all rows
else:
    top_rows = ceil(rows_to_render / 2)
    bottom_rows = rows_to_render - top_rows
    // render top_rows from start, then divider, then bottom_rows from end
```

Collect the selected rows into `Vec<Vec<String>>` (outer = rows, inner = columns).
Each cell is the stringified value or the null placeholder.

### 3. Column width computation

For each column, compute:

```
col_width[c] = max(
    render_width(column_name),
    render_width(type_name),
    max over selected rows of render_width(cell_value[c])
)
```

Where `render_width` uses the `unicode-width` crate's `UnicodeWidthStr::width()`.

**Total render width:**
```
total = 1 + sum(col_width[c] + 3 for each column)
//      │         │            └─ " " + value + " │"
//      └─ leading "│"
```

### 4. Column shrinking

If `total > max_width`:

1. **Shrink phase:** For each column wider than `max_col_width`, compute how
   much it can shrink. Distribute the required shrinkage equally across the
   widest columns first (shrink the widest down to the second-widest, then
   both down to the third-widest, etc.) until we fit or all are at
   `max_col_width`.

2. **Prune phase:** If still too wide after shrinking, add a `…` placeholder
   column (3 + 1 = 4 chars) and remove columns from the middle outward in
   zig-zag order (col N/2, N/2-1, N/2+1, ...) until we fit. After pruning,
   redistribute any leftover space back to remaining shortened columns.

### 5. Footer computation

```
row_count_str   = "{N} rows"         // or "{N} row" if 1
column_count_str = "{M} columns"     // or "{M} column" if 1
shown_str       = "{K} shown"        // only if rows were truncated

// if columns were pruned:
column_count_str += " ({visible} shown)"
```

Footer is rendered only when: rows were truncated, columns were pruned, or
row count ≥ threshold (DuckDB shows it at ≥20 rows; we show it whenever there
are hidden rows or columns, matching DuckDB behavior).

### 6. Rendering

Rendering proceeds line by line into a `String`:

```
┌─────────┬───────┐        ← top border
│  name   │  age  │        ← column names (centered)
│ varchar │ int64 │        ← type names (centered)
├─────────┼───────┤        ← header divider
│ Ada     │    37 │        ← data rows (alignment per type)
│ Linus   │    54 │
│    ·    │    ·  │        ← divider rows (only if truncated)
│    ·    │    ·  │
│    ·    │    ·  │
│ Guido   │    63 │        ← bottom data rows
├─────────┴───────┤        ← footer top border
│     50 rows     │        ← footer text (centered)
│   (40 shown)    │        ← shown count (centered, if truncated)
└─────────────────┘        ← bottom border
```

**Layout characters:**

| Position | Char |
|----------|------|
| Top-left | `┌` |
| Top-right | `┐` |
| Top junction | `┬` |
| Left junction | `├` |
| Right junction | `┤` |
| Cross junction | `┼` |
| Bottom-left | `└` |
| Bottom-right | `┘` |
| Bottom junction | `┴` |
| Horizontal | `─` |
| Vertical | `│` |
| Ellipsis | `…` |
| Dot (divider) | `·` |

**Rendering a value cell:**

```rust
fn render_cell(value: &str, width: usize, alignment: Alignment) -> String {
    let render_w = UnicodeWidthStr::width(value);
    let padding = width - render_w;
    match alignment {
        Left   => format!(" {}{} ", value, " ".repeat(padding)),
        Right  => format!(" {}{} ", " ".repeat(padding), value),
        Center => {
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            format!(" {}{}{} ", " ".repeat(left_pad), value, " ".repeat(right_pad))
        }
    }
}
```

**Rendering a horizontal line:**

```rust
fn render_line(widths: &[usize], left: &str, mid: &str, right: &str, fill: &str) -> String {
    // e.g. render_line(&widths, "├", "┼", "┤", "─")
    // → "├─────────┼───────┤"
    let segments: Vec<String> = widths.iter()
        .map(|&w| fill.repeat(w + 2))  // +2 for padding spaces
        .collect();
    format!("{}{}{}", left, segments.join(mid), right)
}
```

---

## Dependencies

| Crate | Purpose | Status |
|-------|---------|--------|
| `unicode-width` | `UnicodeWidthStr::width()` for grapheme render widths | **New dependency** |
| `terminal_size` | Auto-detect terminal width | **New dependency** |
| `duckdb::arrow` | `RecordBatch`, `Schema`, `DataType`, array access | Already in tree |

Both new crates are small, widely used (60M+ and 30M+ downloads respectively),
and have no transitive dependencies beyond libc.

---

## Test strategy

### Unit tests (inside `src/duckbox.rs`)

Test the renderer in isolation with hand-built `RecordBatch` data:

| Test | What it validates |
|------|-------------------|
| `basic_render` | Simple 2-col, 2-row table matches expected box output |
| `type_display` | Each Arrow DataType maps to correct DuckDB type string |
| `alignment` | Numbers right-aligned, strings left-aligned, headers centered |
| `null_values` | NULL cells render as configured placeholder |
| `utf8_widths` | CJK / emoji characters get correct column width |
| `value_truncation` | Long values truncated with `…` |
| `row_truncation` | >max_rows shows top/bottom split with `·` divider |
| `row_truncation_threshold` | max_rows+3 or fewer rows renders all (no divider) |
| `column_shrinking` | Wide columns shrink equally when table exceeds max_width |
| `column_pruning` | Too many columns prunes from middle, shows `…` column |
| `footer` | Row count, shown count, column count render correctly |
| `single_row` | No footer for small results |
| `empty_result` | Zero rows renders header + empty body + footer |
| `single_column` | Renders correctly with only one column |

### Integration tests (TOML fixtures)

Update `tests/test_cases/json_pretty.toml` (and add new fixture files) to
assert the new box-drawing output format. Existing `json_pretty.toml` test
expectations change from `+`/`-`/`|` to `┌`/`─`/`│`.

Add a new `tests/test_cases/pretty_rendering.toml`:

```toml
cmd = "{dq} from json | {dq} to pretty"

[[cases]]
name = "type_annotations"
# verify type row appears below column names

[[cases]]
name = "number_alignment"
# verify integers and floats are right-aligned

[[cases]]
name = "null_display"
# verify NULL renders as "NULL"

[[cases]]
name = "row_truncation"
# 50+ rows, verify · · · divider and "(N shown)" footer

[[cases]]
name = "wide_values"
# long string values get truncated with …
```

---

## Implementation order

### Step 1: Scaffold `src/duckbox.rs` and type mapping

- Add `unicode-width` and `terminal_size` to `Cargo.toml`
- Create `src/duckbox.rs` with `Config`, `DuckBox`, `Alignment`, `ColumnMeta`
- Implement Arrow `DataType` → DuckDB type name mapping
- Implement Arrow `DataType` → `Alignment` mapping
- Unit tests for type display and alignment

### Step 2: Data extraction

- Implement `RecordBatch` → `Vec<Vec<String>>` cell extraction
- Handle NULL values with configured placeholder
- Implement render width calculation using `unicode-width`
- Unit tests for cell extraction and NULL handling

### Step 3: Column width computation

- Compute column widths from header + data
- Compute total render width
- Unit tests for width computation

### Step 4: Basic rendering (no truncation)

- Implement `render_line()` for horizontal borders
- Implement `render_cell()` for value padding/alignment
- Implement full render pipeline: top border → header → divider → data → bottom border
- Unit tests for basic rendering

### Step 5: Row truncation

- Implement top/bottom row split logic
- Render `·` divider rows between top and bottom sections
- Implement footer rendering (row count, shown count)
- Unit tests for row truncation and footer

### Step 6: Column shrinking and pruning

- Implement equal-shrink algorithm for wide columns
- Implement zig-zag column pruning from middle
- Implement `…` placeholder column
- Implement column count in footer ("N columns (M shown)")
- Unit tests for shrinking and pruning

### Step 7: Terminal width detection

- Wire up `terminal_size` for auto-detection
- Fall back to 120 when not a terminal (piped output)
- Config plumbing from `DuckBox::new()`

### Step 8: Integration

- Wire `DuckBox` into `cmd.rs`, replacing `pretty_format_batches`
- Update `json_pretty.toml` fixture expectations to box-drawing format
- Add `pretty_rendering.toml` integration tests
- Manual smoke testing with real data files

### Step 9: Cleanup

- Run `cargo clippy`, fix warnings
- Verify `cargo test` passes
- Verify TTY auto-detection still works (`select`/`where` auto-pretty)
- Test with narrow terminal widths (80 cols)
- Test with wide Unicode data (CJK, emoji)
