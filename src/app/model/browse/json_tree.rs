#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonTree {
    lines: Vec<TreeLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeLine {
    pub depth: usize,
    pub key: Option<String>,
    pub value: TreeValue,
    pub collapsed: bool,
    pub line_type: LineType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineType {
    ObjectOpen,
    ObjectClose,
    ArrayOpen,
    ArrayClose,
    KeyValue,
    ArrayItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    ObjectOpen { child_count: usize },
    ArrayOpen { child_count: usize },
    Closing,
}

impl JsonTree {
    pub fn new(lines: Vec<TreeLine>) -> Self {
        Self { lines }
    }

    pub fn lines(&self) -> &[TreeLine] {
        &self.lines
    }

    pub fn lines_mut(&mut self) -> &mut [TreeLine] {
        &mut self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn toggle_fold(&mut self, line_idx: usize) -> bool {
        if let Some(line) = self.lines.get_mut(line_idx)
            && matches!(line.line_type, LineType::ObjectOpen | LineType::ArrayOpen)
        {
            line.collapsed = !line.collapsed;
            return true;
        }
        false
    }

    pub fn fold_all(&mut self) {
        for line in &mut self.lines {
            if matches!(line.line_type, LineType::ObjectOpen | LineType::ArrayOpen) {
                line.collapsed = true;
            }
        }
    }

    pub fn unfold_all(&mut self) {
        for line in &mut self.lines {
            if matches!(line.line_type, LineType::ObjectOpen | LineType::ArrayOpen) {
                line.collapsed = false;
            }
        }
    }
}

impl TreeLine {
    pub fn is_collapsible(&self) -> bool {
        matches!(self.line_type, LineType::ObjectOpen | LineType::ArrayOpen)
    }
}
