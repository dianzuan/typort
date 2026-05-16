# Phase 1: 直通语义管线实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 typort 从"帧文本提取"（Phase 0）升级为"语义结构保留"管线。编译 Typst → 获取 HtmlDocument DOM → 按 tag 分发映射 → 生成结构化 OOXML .docx。

**Architecture:** `typst::compile::<HtmlDocument>(world)` → 遍历 DOM 树（`<h2>`→Heading, `<p>`→paragraph, `<strong>`→bold, `<table>`→table）→ OOXML XML via quick-xml → ZIP .docx

**Tech Stack:** typst 0.14.2, typst-html 0.14.2, quick-xml 0.37, zip 2.x

**已验证的 API 关键点：**
- `compile::<HtmlDocument>()` 成功返回语义 DOM
- tag 格式为 `<<h2>>`/`<<p>>`/`<<ol>>` 等（Display trait 输出带双角括号）
- DOM children 包含 `HtmlNode::Element`/`Text`/`Frame`/`Tag` 四种
- `HtmlElement` 有 `.tag`（HtmlTag）、`.children`（EcoVec<HtmlNode>）、`.attrs`（HtmlAttrs）
- complex_paper.typ 生成 219 个顶层语义元素，结构完整

---

## 文件规划

| 文件 | 职责 | 动作 |
|------|------|------|
| `crates/typort-core/src/convert.rs` | DOM → OOXML Document 转换器 | 重写 |
| `crates/typort-core/src/world.rs` | TyportWorld + compile 函数 | 修改（增加 html compile） |
| `crates/typort-core/src/lib.rs` | 模块导出 | 修改 |
| `crates/typort-ooxml/src/document.rs` | Document 模型（增加标题/格式/表格等） | 扩展 |
| `crates/typort-ooxml/src/writer.rs` | OOXML writer（增加 styles.xml/fontTable.xml） | 扩展 |
| `crates/typort-ooxml/src/styles.rs` | styles.xml 生成 | 新建 |
| `crates/typort-cli/src/main.rs` | CLI 切换到新管线 | 修改 |
| `tests/fixtures/complex_paper.typ` | 验收测试输入 | 已有 |

---

### Task 1: 扩展 OOXML Document 模型

**Files:** `crates/typort-ooxml/src/document.rs`

目标：Document 模型支持标题级别、行内格式（粗体/斜体）、列表、基本表格。

- [ ] **Step 1: 写测试——Document 能承载标题和格式化段落**

```rust
#[test]
fn document_with_heading_and_formatted_text() {
    let mut doc = Document::new();
    
    let mut heading = Paragraph::new();
    heading.style = Some(ParagraphStyle::Heading(1));
    heading.add_run("一、引言");
    doc.add_paragraph(heading);
    
    let mut para = Paragraph::new();
    let mut bold_run = Run::new("数字经济");
    bold_run.bold = true;
    para.runs.push(bold_run);
    para.runs.push(Run::new("显著提升创新效率"));
    doc.add_paragraph(para);
    
    assert_eq!(doc.body.elements.len(), 2);
}
```

- [ ] **Step 2: 扩展 document.rs 模型**

```rust
#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub font_family: Option<String>,
    pub font_size_half_pt: Option<u32>,  // Word uses half-points (10.5pt = 21)
}

impl Run {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            font_family: None,
            font_size_half_pt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParagraphStyle {
    Normal,
    Heading(u8),  // 1-5
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    pub style: Option<ParagraphStyle>,
}
```

- [ ] **Step 3: 运行测试确认通过**
- [ ] **Step 4: Commit**

---

### Task 2: 扩展 OOXML Writer（styles.xml + 标题样式 + 格式化 Run）

**Files:** `crates/typort-ooxml/src/writer.rs`, `crates/typort-ooxml/src/styles.rs`

- [ ] **Step 1: 写测试——生成的 docx 包含 styles.xml 和 Heading 样式**

```rust
#[test]
fn docx_with_heading_has_styles_and_pstyle() {
    let mut doc = Document::new();
    let mut h = Paragraph::new();
    h.style = Some(ParagraphStyle::Heading(1));
    h.add_run("测试标题");
    doc.add_paragraph(h);

    let mut buf = Vec::new();
    write_docx(&doc, Cursor::new(&mut buf)).unwrap();

    let mut zip = ZipArchive::new(Cursor::new(&buf)).unwrap();
    
    // styles.xml must exist
    assert!(zip.file_names().any(|n| n == "word/styles.xml"));
    
    // document.xml must reference Heading1 style
    let doc_xml = read_to_string(zip.by_name("word/document.xml").unwrap()).unwrap();
    assert!(doc_xml.contains("w:pStyle"));
    assert!(doc_xml.contains("Heading1"));
}
```

- [ ] **Step 2: 创建 styles.rs — 生成 styles.xml**

生成包含 Normal、Heading1-5、FootnoteText 样式定义的 `styles.xml`。
CJK 字体设置：Normal 用宋体 + Times New Roman，Heading 用黑体。

- [ ] **Step 3: writer.rs 中增加 styles.xml 和 fontTable.xml 输出**

在 `write_docx` 中增加：
```rust
zip.start_file("word/styles.xml", options)?;
zip.write_all(&xml_part(generate_styles)?)?;

zip.start_file("word/fontTable.xml", options)?;
zip.write_all(&xml_part(generate_font_table)?)?;
```

更新 `[Content_Types].xml` 增加对 styles 和 fontTable 的 Override。

- [ ] **Step 4: writer.rs 中 write_paragraph 支持 pStyle**

```rust
if let Some(style) = &para.style {
    w.create_element("w:pPr").write_inner_content(|ppr| {
        let style_id = match style {
            ParagraphStyle::Heading(n) => format!("Heading{n}"),
            ParagraphStyle::Normal => "Normal".to_string(),
        };
        ppr.create_element("w:pStyle")
            .with_attribute(("w:val", style_id.as_str()))
            .write_empty()?;
        Ok(())
    })?;
}
```

- [ ] **Step 5: writer.rs 中 write_run 支持 bold/italic/font**

```rust
// 如果有任何格式属性，生成 w:rPr
if run.bold || run.italic || run.font_family.is_some() || run.font_size_half_pt.is_some() {
    w.create_element("w:rPr").write_inner_content(|rpr| {
        if run.bold { rpr.create_element("w:b").write_empty()?; }
        if run.italic { rpr.create_element("w:i").write_empty()?; }
        if let Some(font) = &run.font_family {
            rpr.create_element("w:rFonts")
                .with_attribute(("w:eastAsia", font.as_str()))
                .write_empty()?;
        }
        if let Some(size) = run.font_size_half_pt {
            rpr.create_element("w:sz")
                .with_attribute(("w:val", &size.to_string()))
                .write_empty()?;
        }
        Ok(())
    })?;
}
```

- [ ] **Step 6: 运行全部测试**
- [ ] **Step 7: Commit**

---

### Task 3: 重写 convert.rs — DOM → OOXML Document 映射

**Files:** `crates/typort-core/src/convert.rs`

核心任务：遍历 HtmlDocument DOM 树，按 tag 类型分发生成 OOXML Document 结构。

- [ ] **Step 1: 写测试——hello.typ 转换后有 heading 和段落**

```rust
#[test]
fn hello_typ_produces_heading_and_paragraphs() {
    let world = TyportWorld::new(Path::new("../../tests/fixtures/hello.typ")).unwrap();
    let doc = convert_html(&world).unwrap();
    
    // Should have heading + 2 paragraphs
    assert!(doc.body.elements.len() >= 3);
    
    // First element should be a heading
    if let BlockElement::Paragraph(p) = &doc.body.elements[0] {
        assert!(matches!(p.style, Some(ParagraphStyle::Heading(1))));
    } else {
        panic!("first element should be heading paragraph");
    }
}
```

- [ ] **Step 2: 实现 convert_html 函数**

```rust
use typst_html::{HtmlDocument, HtmlNode, HtmlElement};

pub fn convert_html(world: &TyportWorld) -> Result<Document, Vec<String>> {
    let result = typst::compile::<HtmlDocument>(world);
    let html_doc = match result.output {
        Ok(doc) => doc,
        Err(errors) => return Err(errors.iter().map(|e| e.message.to_string()).collect()),
    };
    
    let mut doc = Document::new();
    let body = find_body(&html_doc.root);
    convert_children(&body.children, &mut doc);
    Ok(doc)
}

fn convert_children(children: &[HtmlNode], doc: &mut Document) {
    for child in children {
        match child {
            HtmlNode::Element(elem) => convert_element(elem, doc),
            HtmlNode::Text(text, _) => { /* 顶层 text 忽略或加入当前段落 */ }
            HtmlNode::Tag(_) => { /* introspection markers, 暂时跳过 */ }
            HtmlNode::Frame(_) => { /* 布局帧（如数学），Phase 3 处理 */ }
        }
    }
}

fn convert_element(elem: &HtmlElement, doc: &mut Document) {
    let tag = format!("{}", elem.tag);
    match tag.as_str() {
        "<<h2>>" => convert_heading(elem, doc, 1),
        "<<h3>>" => convert_heading(elem, doc, 2),
        "<<h4>>" => convert_heading(elem, doc, 3),
        "<<h5>>" => convert_heading(elem, doc, 4),
        "<<h6>>" => convert_heading(elem, doc, 5),
        "<<p>>" => convert_paragraph(elem, doc),
        "<<strong>>" => { /* 处理为行内格式 */ }
        "<<em>>" => { /* 处理为行内格式 */ }
        "<<ol>>" | "<<ul>>" => convert_list(elem, doc),
        "<<table>>" => { /* Phase 2 */ }
        _ => {
            // 未知 tag，递归处理 children
            convert_children(&elem.children, doc);
        }
    }
}
```

- [ ] **Step 3: 实现 convert_heading / convert_paragraph / inline formatting**

关键：段落内的 children 可能是 `<strong>`/`<em>` 嵌套 + text nodes。需要递归收集 runs：

```rust
fn collect_runs(children: &[HtmlNode], runs: &mut Vec<Run>, bold: bool, italic: bool) {
    for child in children {
        match child {
            HtmlNode::Text(text, _) => {
                let mut run = Run::new(text.as_str());
                run.bold = bold;
                run.italic = italic;
                runs.push(run);
            }
            HtmlNode::Element(elem) => {
                let tag = format!("{}", elem.tag);
                let new_bold = bold || tag == "<<strong>>" || tag == "<<b>>";
                let new_italic = italic || tag == "<<em>>" || tag == "<<i>>";
                collect_runs(&elem.children, runs, new_bold, new_italic);
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: 运行测试，确认 hello.typ 和 complex_paper.typ 的结构**
- [ ] **Step 5: Commit**

---

### Task 4: 更新 CLI + 集成测试

**Files:** `crates/typort-cli/src/main.rs`, `crates/typort-cli/tests/integration.rs`

- [ ] **Step 1: CLI 切换到 convert_html**

```rust
let doc = typort_core::convert::convert_html(&world).unwrap_or_else(|errors| { ... });
```

- [ ] **Step 2: 写验收测试**

```rust
#[test]
fn complex_paper_has_headings_in_docx() {
    let world = TyportWorld::new(Path::new("../../tests/fixtures/complex_paper.typ")).unwrap();
    let doc = typort_core::convert::convert_html(&world).unwrap();
    
    let mut buf = Vec::new();
    typort_ooxml::write_docx(&doc, Cursor::new(&mut buf)).unwrap();
    
    let mut zip = ZipArchive::new(Cursor::new(&buf)).unwrap();
    let doc_xml = read_to_string(zip.by_name("word/document.xml").unwrap()).unwrap();
    
    assert!(doc_xml.contains("Heading1"), "should have Heading1 style");
    assert!(doc_xml.contains("Heading2"), "should have Heading2 style");
    assert!(doc_xml.contains("w:b"), "should have bold formatting");
    assert!(doc_xml.contains("数字经济"), "should contain paper title text");
    
    // Verify styles.xml exists
    assert!(zip.file_names().any(|n| n == "word/styles.xml"));
}
```

- [ ] **Step 3: 运行 complex_paper.typ 的端到端转换，用 Python 验证输出**

```bash
cargo run -p typort-cli -- tests/fixtures/complex_paper.typ -o /tmp/test.docx
python3 -c "
import zipfile
z = zipfile.ZipFile('/tmp/test.docx')
doc = z.read('word/document.xml').decode()
# 检查关键结构
assert 'w:pStyle' in doc, 'no paragraph styles'
assert 'Heading1' in doc, 'no Heading1'
assert 'w:b' in doc, 'no bold'
print('PASS: document has semantic structure')
"
```

- [ ] **Step 4: Commit**

---

### Task 5: 页面设置（sectPr + pgMar）

**Files:** `crates/typort-ooxml/src/writer.rs`, `crates/typort-ooxml/src/document.rs`

- [ ] **Step 1: Document 增加 page_settings 字段**

```rust
pub struct PageSettings {
    pub width_twips: u32,      // A4 = 11906
    pub height_twips: u32,     // A4 = 16838
    pub margin_top: u32,       // 1440 = 1 inch
    pub margin_bottom: u32,
    pub margin_left: u32,
    pub margin_right: u32,
}

impl Default for PageSettings {
    fn default() -> Self {
        // A4 with 2.54cm margins
        Self {
            width_twips: 11906,
            height_twips: 16838,
            margin_top: 1440,
            margin_bottom: 1440,
            margin_left: 1800,
            margin_right: 1800,
        }
    }
}
```

- [ ] **Step 2: writer.rs 在 w:body 末尾生成 w:sectPr**

```rust
body_w.create_element("w:sectPr").write_inner_content(|sect| {
    sect.create_element("w:pgSz")
        .with_attribute(("w:w", &settings.width_twips.to_string()))
        .with_attribute(("w:h", &settings.height_twips.to_string()))
        .write_empty()?;
    sect.create_element("w:pgMar")
        .with_attribute(("w:top", &settings.margin_top.to_string()))
        .with_attribute(("w:bottom", &settings.margin_bottom.to_string()))
        .with_attribute(("w:left", &settings.margin_left.to_string()))
        .with_attribute(("w:right", &settings.margin_right.to_string()))
        .write_empty()?;
    Ok(())
})?;
```

- [ ] **Step 3: 测试验证**
- [ ] **Step 4: Commit**

---

### Task 6: 最终验收 + 代码整理

- [ ] **Step 1: 运行 complex_paper.typ 端到端转换**
- [ ] **Step 2: 用 Python 做完整 14 项检查（对比 Phase 0 的 0/14）**

```python
checks = {
    "Heading styles (w:pStyle)": 'w:pStyle' in doc,
    "Bold (w:b)": '<w:b/>' in doc or '<w:b ' in doc,
    "Italic (w:i)": '<w:i/>' in doc or '<w:i ' in doc,
    "Section properties (w:sectPr)": 'w:sectPr' in doc,
    "Page margins (w:pgMar)": 'w:pgMar' in doc,
    "Paragraph count > 10": doc.count('<w:p>') > 10,
    "styles.xml exists": 'word/styles.xml' in names,
    "fontTable.xml exists": 'word/fontTable.xml' in names,
}
```

目标：至少 6/14 通过（对比 Phase 0 的 0/14）。

- [ ] **Step 3: cargo clippy + fmt + test**
- [ ] **Step 4: 移除 Phase 0 的旧 convert.rs 帧遍历代码（或保留为 convert_legacy.rs）**
- [ ] **Step 5: 更新 CLAUDE.md 反映新架构**
- [ ] **Step 6: Final commit**

---

## 验收标准（Phase 1 完成条件）

在 `complex_paper.typ` 上测试：

| 检查项 | Phase 0 结果 | Phase 1 目标 |
|--------|-------------|-------------|
| Heading styles | ✗ | ✓ |
| Bold formatting | ✗ | ✓ |
| Italic formatting | ✗ | ✓ |
| Paragraph separation | ✗ | ✓ |
| Section properties | ✗ | ✓ |
| Page margins | ✗ | ✓ |
| styles.xml | ✗ | ✓ |
| fontTable.xml | ✗ | ✓ |
| Footnotes | ✗ | △（Phase 2） |
| Tables | ✗ | △（Phase 2） |
| Math (OMML) | ✗ | △（Phase 3） |
| Lists | ✗ | ✓（基础） |
| Chinese numbering | ✗ | △（Phase 2） |
| Line spacing | ✗ | ✓ |

**最低通过门槛：8/14 项 ✓**
