//! Bounded, typed selection statistics and validated AI chart plans. Never execute model code.
use crate::{
    EngineError, Result, Session,
    cmd::{CommandSpec, cmd, edit, has_doc},
};
use gridcraft_core::{CellRef, MAX_COLS, RangeRef, Value};
use gridcraft_model::{Anchor, Chart, ChartKind, LegendPos, NumFmt, Series, Sheet, Style, Workbook};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

fn err(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Datum {
    pub address: String,
    pub text: String,
    pub kind: String,
    pub number: Option<f64>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnStats {
    pub column: String,
    pub name: String,
    pub numeric_count: usize,
    pub missing_count: usize,
    pub error_count: usize,
    pub text_count: usize,
    pub date_count: usize,
    pub sum: Option<f64>,
    pub mean: Option<f64>,
    pub median: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSnapshot {
    #[serde(skip)]
    pub document: u64,
    #[serde(skip)]
    pub revision: u64,
    #[serde(skip)]
    pub sheet_index: usize,
    pub sheet: String,
    pub range: String,
    pub has_headers: bool,
    pub columns: Vec<ColumnStats>,
    pub rows: Vec<Vec<Datum>>,
    pub duplicate_rows: usize,
    pub hidden_rows: usize,
    pub notes: Vec<String>,
}
impl TableSnapshot {
    pub fn label(&self) -> String {
        format!("{}!{} · {} 行 × {} 列", self.sheet, self.range, self.rows.len(), self.columns.len())
    }
    pub fn params(&self, plan: &ChartPlan, on_source: bool) -> Json {
        json!({"document":self.document,"revision":self.revision,"sheet":self.sheet_index,"range":self.range,"hasHeaders":self.has_headers,"plan":plan,"onSource":on_source})
    }
}

fn total(values: &[f64]) -> Option<f64> {
    let (mut sum, mut correction) = (0.0, 0.0);
    for &v in values {
        let next = sum + v;
        correction += if sum.abs() >= v.abs() { (sum - next) + v } else { (v - next) + sum };
        sum = next;
    }
    let n = sum + correction;
    n.is_finite().then_some(n)
}

/// Only the selected rectangle is read. Empty cells stay empty; text numbers stay text.
pub fn capture(s: &Session, has_headers: bool) -> Result<TableSnapshot> {
    let d = s.doc()?;
    if d.selection.ranges.len() != 1 {
        return Err(err("智能图表和数据分析需要一个连续矩形选区，请连同表头框选。"));
    }
    let range = *d.selection.ranges.first().ok_or_else(|| err("请先框选数据。"))?;
    capture_range(s, d.sheet_index(), range, has_headers)
}
fn capture_range(s: &Session, sheet_index: usize, range: RangeRef, has_headers: bool) -> Result<TableSnapshot> {
    let d = s.doc()?;
    if !range.start.is_valid()
        || !range.end.is_valid()
        || range.start.row > range.end.row
        || range.start.col > range.end.col
        || range.count() > 10_000
        || range.end.col.saturating_sub(range.start.col) >= 32
    {
        return Err(err("请缩小到 32 列、10000 个格子以内的连续选区；最多发送 1000 个非空单元格。"));
    }
    let sh = d.wb.sheet(sheet_index).ok_or_else(|| err("原工作表不存在。"))?;
    if sh.merges.iter().any(|r| r.intersects(&range)) {
        return Err(err("选区包含合并单元格，无法可靠判断数据行列。请仅选择未合并的数据表。"));
    }
    let first = range.start.row.saturating_add(u32::from(has_headers));
    if first > range.end.row {
        return Err(err("选区只有表头，没有数据行；无表头数据请取消“首行为表头”。"));
    }
    let mut count = 0usize;
    let mut bytes = 0usize;
    let datum = |at: CellRef, count: &mut usize, bytes: &mut usize| -> Result<Datum> {
        let value = sh.value(at);
        if !matches!(value, Value::Empty) {
            *count = count.saturating_add(1);
        }
        let text = crate::display::value_text(&d.wb, sh, at);
        *bytes = bytes.saturating_add(text.len());
        if *count > 1000 || *bytes > 64 * 1024 {
            return Err(err("选区超过 1000 个非空单元格或 64 KB 文字，请分批选择。"));
        }
        if sh.cell_pictures.contains_key(&at) {
            return Err(err(format!("{} 含单元格图片，本功能仅分析文字和数值。", at.a1())));
        }
        let (kind, number) = match value {
            Value::Empty => ("empty", None),
            Value::Number(n) if n.is_finite() => {
                let date = crate::display::number_format(d.wb.styles.get(sh.style_id(at)).num_fmt.as_str()).is_date();
                (if date { "date" } else { "number" }, Some(n))
            }
            Value::Text(t) if t.is_empty() => ("empty", None),
            Value::Text(_) => ("text", None),
            Value::Bool(_) => ("boolean", None),
            _ => ("error", None),
        };
        Ok(Datum { address: at.a1(), text, kind: kind.into(), number })
    };
    let mut columns = Vec::new();
    for col in range.start.col..=range.end.col {
        let at = CellRef::new(range.start.row, col);
        let column = CellRef::new(0, col).a1().trim_end_matches('1').to_string();
        let name = if has_headers { datum(at, &mut count, &mut bytes)?.text } else { column.clone() };
        columns.push(ColumnStats { column: column.clone(), name: if name.is_empty() { column } else { name }, ..Default::default() });
    }
    let mut rows = Vec::new();
    let mut unique = BTreeSet::new();
    let mut duplicate_rows = 0;
    let mut hidden_rows = 0;
    for row in first..=range.end.row {
        let mut cells = Vec::new();
        let mut key = Vec::new();
        for col in range.start.col..=range.end.col {
            let cell = datum(CellRef::new(row, col), &mut count, &mut bytes)?;
            key.push((cell.kind.clone(), cell.text.clone()));
            cells.push(cell);
        }
        if cells.iter().any(|c| c.kind != "empty") && !unique.insert(key) {
            duplicate_rows += 1;
        }
        if sh.is_row_hidden(row) {
            hidden_rows += 1;
        }
        rows.push(cells);
    }
    if count == 0 {
        return Err(err("选区没有数据。"));
    }
    let mut notes = vec!["统计由本机按列计算；日期不作金额求和，文本数字不自动转换。空白不当作 0；合计行未自动排除。".into()];
    if hidden_rows > 0 {
        notes.push(format!("包含 {hidden_rows} 个隐藏/筛选行，统计和图表都包含这些行。"));
    }
    for (index, stat) in columns.iter_mut().enumerate() {
        let mut values = Vec::new();
        for row in &rows {
            if let Some(cell) = row.get(index) {
                match cell.kind.as_str() {
                    "empty" => stat.missing_count += 1,
                    "error" => stat.error_count += 1,
                    "date" => stat.date_count += 1,
                    "number" => {
                        if let Some(n) = cell.number {
                            values.push(n);
                        }
                    }
                    _ => stat.text_count += 1,
                }
            }
        }
        stat.numeric_count = values.len();
        values.sort_by(f64::total_cmp);
        if !values.is_empty() {
            stat.sum = total(&values);
            stat.mean = total(&values.iter().map(|v| v / values.len() as f64).collect::<Vec<_>>());
            stat.min = values.first().copied();
            stat.max = values.last().copied();
            let i = values.len() / 2;
            stat.median =
                if values.len() % 2 == 0 { values.get(i - 1).zip(values.get(i)).map(|(a, b)| a / 2.0 + b / 2.0) } else { values.get(i).copied() };
            if stat.sum.is_none() {
                notes.push(format!("{} 列求和超出有限数值范围，未提供总计。", stat.column));
            }
        }
    }
    let table = TableSnapshot {
        document: d.uid,
        revision: d.revision,
        sheet_index,
        sheet: sh.name.to_string(),
        range: range.a1(),
        has_headers,
        columns,
        rows,
        duplicate_rows,
        hidden_rows,
        notes,
    };
    if serde_json::to_vec(&table).map_err(|e| err(e.to_string()))?.len() > 160 * 1024 {
        return Err(err("结构化数据超过 160 KB，请缩小选区。"));
    }
    Ok(table)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    Column,
    Bar,
    Line,
    Pie,
    Doughnut,
    Scatter,
}
impl ChartType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Column => "柱形图",
            Self::Bar => "条形图",
            Self::Line => "折线图",
            Self::Pie => "饼图",
            Self::Doughnut => "环形图",
            Self::Scatter => "散点图",
        }
    }
    fn kind(self) -> ChartKind {
        match self {
            Self::Column => ChartKind::ColumnClustered,
            Self::Bar => ChartKind::BarClustered,
            Self::Line => ChartKind::Line,
            Self::Pie => ChartKind::Pie,
            Self::Doughnut => ChartKind::Doughnut,
            Self::Scatter => ChartKind::Scatter,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Aggregation {
    #[default]
    None,
    Sum,
    Mean,
    Count,
}
impl Aggregation {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "不汇总",
            Self::Sum => "求和",
            Self::Mean => "平均值",
            Self::Count => "有效数值计数",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    Source,
    CategoryAsc,
    ValueAsc,
    ValueDesc,
}
impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "原始顺序",
            Self::CategoryAsc => "分类升序",
            Self::ValueAsc => "数值升序",
            Self::ValueDesc => "数值降序",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartPlan {
    pub title: String,
    pub chart_type: ChartType,
    pub x_column: String,
    pub y_columns: Vec<String>,
    #[serde(default)]
    pub aggregation: Aggregation,
    #[serde(default)]
    pub sort: Sort,
    pub reason: String,
    #[serde(default)]
    pub x_title: String,
    #[serde(default)]
    pub y_title: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PreparedChart {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Json>>,
    pub notes: Vec<String>,
}

/// Resolve only whitelisted column identifiers, never model-generated ranges/formulas/values.
pub fn prepare(table: &TableSnapshot, plan: &ChartPlan) -> Result<PreparedChart> {
    if plan.title.trim().is_empty()
        || plan.title.chars().count() > 120
        || plan.reason.len() > 4096
        || plan.x_title.chars().count() > 120
        || plan.y_title.chars().count() > 120
    {
        return Err(err("图表标题或推荐说明不符合长度限制。"));
    }
    if plan.y_columns.is_empty() || plan.y_columns.len() > 6 {
        return Err(err("请选择 1～6 个数值系列。"));
    }
    let find = |name: &str| table.columns.iter().position(|c| c.column == name).ok_or_else(|| err(format!("AI 引用了选区外的列：{name}")));
    let x = find(&plan.x_column)?;
    let ys = plan.y_columns.iter().map(|c| find(c)).collect::<Result<Vec<_>>>()?;
    if ys.contains(&x) || ys.iter().collect::<BTreeSet<_>>().len() != ys.len() {
        return Err(err("分类列与数值列不能重复。"));
    }
    if plan.chart_type == ChartType::Scatter && (plan.aggregation != Aggregation::None || plan.sort != Sort::Source) {
        return Err(err("散点图需保留原始配对数据，不能分组或排序。"));
    }
    let mut rows = Vec::<Vec<Json>>::new();
    let mut keys = Vec::<(String, Option<f64>)>::new();
    let mut skipped = 0;
    for row in &table.rows {
        if row.iter().all(|c| c.kind == "empty") {
            skipped += 1;
            continue;
        }
        let category = row.get(x).ok_or_else(|| err("数据列不完整。"))?;
        if category.kind == "empty" || category.kind == "error" {
            return Err(err(format!("{} 分类/横轴数据为空或有错误，请先补齐。", category.address)));
        }
        if plan.chart_type == ChartType::Scatter && category.kind != "number" {
            return Err(err(format!("{} 不是散点图需要的数值横轴。", category.address)));
        }
        let mut values = vec![if plan.chart_type == ChartType::Scatter { json!(category.number) } else { json!(category.text) }];
        for &y in &ys {
            let cell = row.get(y).ok_or_else(|| err("数值列不完整。"))?;
            if cell.kind != "empty" && cell.kind != "number" {
                return Err(err(format!("{} 不是数值（文本数字、日期或错误不会当作 0）。", cell.address)));
            }
            if plan.chart_type == ChartType::Scatter && cell.number.is_none() {
                return Err(err(format!("{} 缺少散点图数值，请先补齐。", cell.address)));
            }
            values.push(json!(cell.number));
        }
        keys.push((category.text.clone(), category.number));
        rows.push(values);
    }
    let mut notes = vec!["图表引用新建辅助表中的数值快照；源数据后来变化时需要重新生成，不会自动刷新。".into()];
    if skipped > 0 {
        notes.push(format!("跳过 {skipped} 个完全空白行。"));
    }
    if plan.aggregation != Aggregation::None {
        let mut groups = BTreeMap::<String, usize>::new();
        let mut grouped = Vec::<Vec<Vec<f64>>>::new();
        let mut group_keys = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let key = keys.get(i).ok_or_else(|| err("分组数据缺失。"))?;
            let token = format!("{}:{}", key.1.map_or("text".into(), |n| n.to_string()), key.0);
            let at = if let Some(&at) = groups.get(&token) {
                at
            } else {
                let at = grouped.len();
                groups.insert(token, at);
                grouped.push(vec![Vec::new(); ys.len()]);
                group_keys.push(key.clone());
                at
            };
            for (j, value) in row.iter().skip(1).enumerate() {
                if let Some(n) = value.as_f64()
                    && let Some(bucket) = grouped.get_mut(at).and_then(|g| g.get_mut(j))
                {
                    bucket.push(n);
                }
            }
        }
        rows = grouped
            .iter()
            .enumerate()
            .map(|(i, series)| {
                let mut row = vec![json!(group_keys.get(i).map(|k| k.0.clone()).unwrap_or_default())];
                for values in series {
                    let n = if plan.aggregation == Aggregation::Count {
                        Some(values.len() as f64)
                    } else if values.is_empty() {
                        None
                    } else {
                        match plan.aggregation {
                            Aggregation::Count => Some(values.len() as f64),
                            Aggregation::Mean => total(&values.iter().map(|v| v / values.len() as f64).collect::<Vec<_>>()),
                            _ => total(values),
                        }
                    };
                    if !values.is_empty() && n.is_none() {
                        return Err(err("分组汇总溢出，无法安全生成图表。"));
                    }
                    row.push(json!(n));
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>>>()?;
        keys = group_keys;
        notes.push(format!("分组方式：{}；计数仅统计有效数值。", plan.aggregation.label()));
    }
    if rows.len() < 2 || rows.len() > 500 {
        return Err(err("图表需要 2～500 个数据点，请调整选区或分组方式。"));
    }
    if rows.iter().all(|r| r.iter().skip(1).all(Json::is_null)) {
        return Err(err("数值列没有有效数据。"));
    }
    let pie = matches!(plan.chart_type, ChartType::Pie | ChartType::Doughnut);
    if pie
        && (ys.len() != 1
            || rows.len() > 8
            || rows.iter().any(|r| r.get(1).and_then(Json::as_f64).is_none_or(|n| n < 0.0))
            || total(&rows.iter().filter_map(|r| r.get(1).and_then(Json::as_f64)).collect::<Vec<_>>()).is_none_or(|n| n <= 0.0))
    {
        return Err(err("饼图/环形图只适合 2～8 个非负类别、一个完整数值系列，且合计须大于 0。"));
    }
    let mut order = (0..rows.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| match plan.sort {
        Sort::Source => a.cmp(&b),
        Sort::CategoryAsc => match (keys.get(a), keys.get(b)) {
            (Some(a), Some(b)) => match (a.1, b.1) {
                (Some(a), Some(b)) => a.total_cmp(&b),
                _ => a.0.cmp(&b.0),
            },
            _ => a.cmp(&b),
        },
        Sort::ValueAsc | Sort::ValueDesc => {
            let a = rows.get(a).and_then(|r| r.get(1)).and_then(Json::as_f64);
            let b = rows.get(b).and_then(|r| r.get(1)).and_then(Json::as_f64);
            match (a, b) {
                (Some(a), Some(b)) => {
                    if plan.sort == Sort::ValueDesc {
                        b.total_cmp(&a)
                    } else {
                        a.total_cmp(&b)
                    }
                }
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Equal,
            }
        }
    });
    rows = order.iter().filter_map(|&i| rows.get(i).cloned()).collect();
    let mut headers = vec![table.columns.get(x).map(|c| c.name.clone()).unwrap_or_default()];
    headers.extend(ys.iter().filter_map(|&y| {
        table
            .columns
            .get(y)
            .map(|c| if plan.aggregation == Aggregation::None { c.name.clone() } else { format!("{}（{}）", c.name, plan.aggregation.label()) })
    }));
    Ok(PreparedChart { headers, rows, notes })
}

fn validate_current(s: &Session, p: &Json) -> Result<TableSnapshot> {
    let d = s.doc()?;
    if p.get("document").and_then(Json::as_u64) != Some(d.uid)
        || p.get("revision").and_then(Json::as_u64) != Some(d.revision)
        || p.get("sheet").and_then(Json::as_u64) != Some(d.sheet_index() as u64)
    {
        return Err(err("工作簿或数据已经变化，请返回原工作表并重新分析/生成图表。"));
    }
    let range = p.get("range").and_then(Json::as_str).and_then(RangeRef::parse).ok_or_else(|| err("缺少原选区。"))?;
    capture_range(s, d.sheet_index(), range, p.get("hasHeaders").and_then(Json::as_bool).unwrap_or(true))
}
fn plan_param(p: &Json) -> Result<ChartPlan> {
    serde_json::from_value(p.get("plan").cloned().ok_or_else(|| err("缺少图表方案。"))?).map_err(|e| err(format!("无效图表方案：{e}")))
}

fn result_sheet(wb: &mut Workbook, table: &TableSnapshot, plan: &ChartPlan) -> Result<(Sheet, Chart)> {
    let prepared = prepare(table, plan)?;
    // Preserve a uniform numeric format (especially percentages) without copying source fonts,
    // protection, fills or borders into the new helper sheet. Counts use plain numbers.
    let source = wb.sheet(table.sheet_index).ok_or_else(|| err("原工作表不存在。"))?;
    let formats = std::iter::once(&plan.x_column)
        .chain(plan.y_columns.iter())
        .enumerate()
        .map(|(index, column)| {
            if index > 0 && plan.aggregation == Aggregation::Count {
                return None;
            }
            let pos = table.columns.iter().position(|c| &c.column == column)?;
            let codes = table
                .rows
                .iter()
                .filter_map(|r| r.get(pos))
                .filter(|c| c.kind == "number")
                .filter_map(|c| CellRef::parse(&c.address))
                .map(|at| wb.styles.get(source.style_id(at)).num_fmt.as_str().to_string())
                .collect::<BTreeSet<_>>();
            (codes.len() == 1).then(|| codes.into_iter().next()).flatten()
        })
        .collect::<Vec<_>>();
    let styles = formats
        .into_iter()
        .map(|code| code.map(|code| wb.styles.intern(Style { num_fmt: NumFmt::new(&code), ..Default::default() })))
        .collect::<Vec<_>>();
    let name = (1..=10000).map(|i| format!("AI图表{i}")).find(|n| wb.sheet_index(n).is_none()).ok_or_else(|| err("AI 图表工作表数量过多。"))?;
    let mut sh = Sheet::new(name.clone());
    sh.set_value(CellRef::new(0, 0), Value::text(plan.title.as_str()));
    sh.set_value(CellRef::new(1, 0), Value::text(format!("来源：{}!{}；数值快照，不自动刷新", table.sheet, table.range)));
    sh.set_value(CellRef::new(2, 0), Value::text(format!("{}；{}", plan.reason, prepared.notes.join("；"))));
    for (col, header) in prepared.headers.iter().enumerate() {
        sh.set_value(CellRef::new(4, col as u32), Value::text(header.as_str()));
    }
    for (row, values) in prepared.rows.iter().enumerate() {
        for (col, v) in values.iter().enumerate() {
            let value = if let Some(n) = v.as_f64() {
                Value::Number(n)
            } else if let Some(t) = v.as_str() {
                Value::text(t)
            } else {
                Value::Empty
            };
            sh.set_value(CellRef::new(row as u32 + 5, col as u32), value);
            if v.as_f64().is_some()
                && let Some(style) = styles.get(col).and_then(|s| *s)
            {
                sh.set_style(CellRef::new(row as u32 + 5, col as u32), style);
            }
        }
    }
    let qualified = |col: u32, start: u32, end: u32| {
        format!("{}!{}", gridcraft_formula::quote_sheet(&name), RangeRef::new(CellRef::new(start, col), CellRef::new(end, col)).a1())
    };
    let end = prepared.rows.len() as u32 + 4;
    let series = (1..prepared.headers.len())
        .map(|col| Series {
            name: Some(qualified(col as u32, 4, 4)),
            values: qualified(col as u32, 5, end),
            categories: Some(qualified(0, 5, end)),
            ..Default::default()
        })
        .collect();
    let source = format!(
        "{}!{}",
        gridcraft_formula::quote_sheet(&name),
        RangeRef::new(CellRef::new(4, 0), CellRef::new(end, prepared.headers.len() as u32 - 1)).a1()
    );
    let chart = Chart {
        id: wb.next_object_id(),
        kind: plan.chart_type.kind(),
        anchor: Anchor {
            cell: CellRef::new(4, prepared.headers.len() as u32 + 2),
            dx: 0.0,
            dy: 0.0,
            width: 600.0,
            height: 360.0,
            mode: Default::default(),
        },
        title: Some(plan.title.clone()),
        series,
        legend: LegendPos::Bottom,
        data_labels: matches!(plan.chart_type, ChartType::Pie | ChartType::Doughnut),
        gridlines: !matches!(plan.chart_type, ChartType::Pie | ChartType::Doughnut),
        style: 1,
        x_title: (!plan.x_title.is_empty()).then(|| plan.x_title.clone()),
        y_title: (!plan.y_title.is_empty()).then(|| plan.y_title.clone()),
        source: Some(source),
        by_rows: false,
        pivot: None,
    };
    Ok((sh, chart))
}
/// Preview in a copy-on-write workbook; does not create an undo step or touch the document.
pub fn preview(s: &Session, p: &Json) -> Result<(Workbook, usize, Chart)> {
    let table = validate_current(s, p)?;
    let plan = plan_param(p)?;
    let mut wb = (*s.doc()?.wb).clone();
    let (sheet, chart) = result_sheet(&mut wb, &table, &plan)?;
    let si = wb.sheets.len();
    wb.sheets.push(Arc::new(sheet));
    Ok((wb, si, chart))
}
fn insert(s: &mut Session, p: &Json) -> Result<Json> {
    let table = validate_current(s, p)?;
    let plan = plan_param(p)?;
    let on_source = p.get("onSource").and_then(Json::as_bool).unwrap_or(true);
    if s.doc()?.wb.protected_structure {
        return Err(err("工作簿结构受保护，不能创建图表辅助表。"));
    }
    if on_source && s.doc()?.wb.sheet(table.sheet_index).is_some_and(Sheet::is_protected) {
        return Err(err("源工作表受保护，请取消“插入当前表”或先解除保护。"));
    }
    let mut workbook = (*s.doc()?.wb).clone();
    let (sheet, mut chart) = result_sheet(&mut workbook, &table, &plan)?;
    let name = sheet.name.to_string();
    let id = chart.id;
    if on_source {
        let range = RangeRef::parse(&table.range).ok_or_else(|| err("原选区无效。"))?;
        chart.anchor.cell = CellRef::new(range.start.row, range.end.col.saturating_add(2).min(MAX_COLS.saturating_sub(1)));
    }
    edit(s, move |cx| {
        cx.wb = workbook;
        let new_index = cx.wb.sheets.len();
        cx.wb.sheets.push(Arc::new(sheet));
        let target = if on_source { table.sheet_index } else { new_index };
        cx.sheet_mut(target)?.charts.push(chart);
        cx.wb.active_sheet = target;
        if !on_source {
            *cx.sel = crate::Selection::default();
        }
        cx.structural = true;
        Ok(json!({"chart":id,"dataSheet":name,"sheet":target,"snapshot":true}))
    })
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "data.aiProfile","AI Data Profile",[],None,"{hasHeaders?: true} → bounded typed selection and deterministic column statistics",has_doc,|s,p|serde_json::to_value(capture(s,p.get("hasHeaders").and_then(Json::as_bool).unwrap_or(true))?).map_err(|e|err(e.to_string()))),
        cmd!(
            "insert.aiChart",
            "Insert AI Chart",
            [],
            None,
            "{document, revision, sheet, range, hasHeaders?, plan:{title,chartType,xColumn,yColumns,aggregation?,sort?,reason,xTitle?,yTitle?},onSource?:true} — validated plan, literal helper data, one undo step",
            has_doc,
            insert
        ),
    ]
}

#[cfg(test)]
#[path = "insights_tests.rs"]
mod tests;
