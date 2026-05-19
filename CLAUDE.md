# typort

通用 Typst → Word (.docx) 转换器。任意合法的 .typ 文件都应正确转换为可编辑的 .docx。

## 核心理念

- **我们做的是格式转换，不是 Word 文档生成。** 输入是 Typst 文档，输出是 Word 文档。不对输入内容做假设，不针对特定文档类型硬编码。
- **双编译策略**：HtmlDocument 提供语义结构（标题、段落、列表、表格、脚注），PagedDocument 提供排版信息（字体、字号、页面尺寸）和恢复 HTML 丢失的内容（如 `#align(center)` 等无 HTML show rule 的元素）。
- **直接生成 OOXML**：不依赖 docx-rs 或中间格式，用 quick-xml 直接生成 Word XML 以保证完全控制。

## 转换能力（当前状态）

### 已实现
- 标题（h1-h5）→ Word Heading 样式
- 段落、粗体、斜体、上标、下标、等宽
- 脚注（含带圈数字格式）
- 数学公式（Typst → OMML）：分数、上下标、根号、求和/积分/连乘、定界符、矩阵、向量、重音符号、上/下划线、命名函数、cases、overbrace/underbrace/overbracket/underbracket/overparen/underparen/overshell/undershell、多行对齐公式（eqArr）
- 表格（含合并单元格 colspan/rowspan）
- 有序/无序列表
- 代码块、引用块、术语列表
- 文档元数据（标题、作者、创建时间）
- 页面尺寸和边距（从渲染结果提取）
- 字体/字号自动检测
- CJK 语言标签、两端对齐、首行缩进
- 首段缩进抑制（标题后首段）
- 参考文献悬挂缩进
- 公式编号（章节感知）
- 期刊预设（页面边距覆盖）
- 图片嵌入（PNG/JPG → DrawingML `w:drawing` + `word/media/`）
- SVG/Typst 矢量图光栅化（resvg → PNG 后嵌入）
- 文档网格（`w:docGrid`）
- 段落控制（`w:keepNext`、`w:widowControl`）
- CJK 排版属性（`w:kinsoku`、`w:overflowPunct`、`w:autoSpaceDE/DN`）
- 交叉引用（`@label` → `w:bookmarkStart` + `REF` 域代码）
- 超链接（`link()` → `w:fldSimple HYPERLINK`）
- 分页符（`#pagebreak()` → `w:br type="page"`，从 PagedDocument 检测页面早结束）
- 分节（页面设置变化 → 多 `w:sectPr`）
- 页眉页脚（从 PagedDocument 边距区域提取文本）
- 分栏（`#page(columns: N)` → `w:cols`）
- 目录域（`#outline()` → `TOC` 域代码）
- 多行对齐公式（`AlignPointElem` + `LinebreakElem` → `m:eqArr`）
- 水平分割线（`#line(length: 100%)` → `w:pBdr` 底部边框，从 PagedDocument Shape 检测）
- 多页 `#align()` 内容恢复（不再仅限首页，全页面扫描缺失内容）

### 未实现（按优先级）

**P1 — 数学公式：**
- ~~矩阵 `mat` → `m:m`~~ ✅
- ~~向量/重音 `accent`/`hat`/`arrow` → `m:acc`~~ ✅
- ~~上/下划线 → `m:bar`~~ ✅
- ~~命名函数 `sin`/`cos`/`lim` → `m:func`~~ ✅
- ~~多行对齐公式 → `m:eqArr`~~ ✅
- ~~`cases` → `m:d` + `m:eqArr`~~ ✅
- ~~花括号注释 `overbrace`/`underbrace` → `m:groupChr`~~ ✅

**P3 — 增强：**
- Ruby 注音（`w:ruby`，Typst 0.14.2 无原生支持）

### 已知限制（OMML 层面）
- OMML 不支持数学内着色（`\color` 无等价物）
- OMML 不支持可伸缩箭头（`\xrightarrow` 无等价物）
- OMML 不支持删除线/cancel（只能用 `m:borderBox` 对角线近似）
- Word 强制数学区域使用 Cambria Math 字体

## Build & Test

```bash
cargo build --workspace        # Build all crates
cargo test --workspace         # Run all tests (163 tests)
cargo run -p typort-cli -- input.typ -o output.docx  # Run CLI
```

## Architecture

Cargo workspace with 5 crates under `crates/`:

- `typort-cli` — Binary. CLI entry point (clap). Depends on typort-core.
- `typort-core` — Lib. Typst compilation (World impl), HtmlDocument 遍历, PagedDocument 恢复, 元素分发. Depends on typort-ooxml, typort-math, typort-presets.
- `typort-ooxml` — Lib. OOXML XML generation (pure quick-xml) + ZIP packaging. Document model → Word XML.
- `typort-math` — Lib. Typst math Content → OMML conversion. 已实现 13/17 OMML 元素 + 独立 eqArr.
- `typort-presets` — Lib. Journal preset TOML loading.

## Key Dependencies

- `typst` 0.14.2 — Compiler crate, provides Content tree
- `typst-kit` 0.14.2 — Font discovery helpers (embedded fonts only, no system fonts)
- `quick-xml` 0.37 — XML serialization (we do NOT use docx-rs)
- `resvg` + `tiny-skia` — SVG rasterization (SVG → PNG for embedding)
- `zip` 2.x — .docx ZIP packaging

## Conventions

- Rust 2024 edition
- `#![warn(clippy::pedantic)]` in all crate roots
- Tests go in-module for unit tests, `crates/typort-cli/tests/` for integration tests
- Test fixtures in `tests/fixtures/`
- No docx-rs — all OOXML XML is generated via quick-xml for full control
- Typst World uses embedded fonts only (no system font dependency for reproducibility)
- 新特性必须有对应测试才算完成
